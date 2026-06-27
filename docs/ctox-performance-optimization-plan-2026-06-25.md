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
- Browser IndexedDB paths still have broad scan risks, while demand-cache
  sidecar invalidation and under-budget eviction scans are reduced;
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
  `rxdb.query.fetch` bounded. Follow-up work now enforces/caps request windows
  server-side and sends frames from stream-capable storage paths through a
  bounded producer/sender bridge, so compiled cursor responses no longer wait
  for the full bounded response set before the first chunk can leave. Complex
  Mango fallback queries can still scan/build candidates before the first
  frame and remain a P0 item.
- Daemon idle work is still dominated by source-stamp or polling cost in some
  loops: configured email sync, recovery/queue probes, and hourly harness
  audit. Notes, generic business-record projection stamps, and desktop-file
  indexing no longer touch heavy payloads or recurse through file roots on every
  short idle tick, but they still need watcher, high-water, or event-driven
  triggering to remove periodic checks entirely.
- Browser IndexedDB/WebRTC remains a P1 workstream: `allDocuments()` fallback,
  local-write push coalescing, browser chunk write granularity, collection
  re-query subscriptions, and chunk bookkeeping remain.
- DB growth needs a real retention/horizon contract, not only pruning:
  physical deletes must respect replication checkpoints, soft-delete forms must
  be measured separately, attachment lifecycles need reference-based retention,
  and WAL/freelist shrink policy must be explicit.

Current verification pass on 2026-06-25 rechecked the plan against the latest
patch state and found two stale status entries:

- The original `M14` WebRTC file-stream blocking finding is now fixed for the
  `rxdb.file.fetch` streaming path: no `futures::executor::block_on` or
  `std::thread::sleep` remains in `file_fetch_handler.rs`, and the native
  Business OS demand file source no longer calls async RxDB from the sync file
  source callback.
- The original `M5` native stale desktop chunk generation prune scan is now
  fixed for the native cleanup path: candidate chunks are selected by a
  deterministic primary-key range over local SQLite, with an `EXPLAIN` guard
  proving `SEARCH ... id>?` rather than a table scan. This does not close the
  separate DB retention/blob-store/browser-consumer work.

Additional targeted subagent review on 2026-06-25 rechecked three risk areas
against source:

- Service/daemon idle loops: status/recovery, durable-queue empty probes,
  channel no-activity backoff, schedule due-task gates, and most Business OS
  projection source stamps are materially improved. The remaining native idle
  risks are desktop-file polling without watchers, provider polling instead of
  IMAP IDLE/delta tokens, per-record RxDB projection upsert metadata checks,
  broad Core-DB/WAL gates, and stuck `pending_sync` command retries. The
  communication-intake part of the Business Record projection stamp has been
  moved to a trigger-maintained projection clock, but Notes/desktop still lack
  watcher/dirty-root triggering.
- Native RxDB/SQLite: simple selectors, counts, read-only connection reads,
  query-fetch window caps, stream-capable compiled query sending, file-fetch
  async backpressure, and native desktop chunk primary-key pruning are real
  reductions. The remaining architecture risks are complex Mango fallback
  scans before first query-fetch frames, polling-style change detection,
  retention without replication horizon, large file materialization into
  in-memory/base64 chunk vectors, and generic blob fetches without the same
  query-plan guards as desktop chunks.
- Browser/RxDB/WebRTC: the WebRTC-only boundary, shared room, active collection
  gating, demand-only pull for `desktop_file_chunks`, file-viewer demand fetch,
  and transport-status coalescing are in place. Primary-key IndexedDB reads,
  schema-index equality/range cursors, browser `count()`, CTOX-origin
  push-scan suppression, and sync diagnostic fanout are now reduced. The
  remaining browser risks are non-indexed `allDocuments()` fallback, full
  re-query subscriptions, per-chunk browser uploads, and chunk bookkeeping.

Additional read-only subagent review on 2026-06-26 rechecked the remaining
performance risks after the browser/RxDB fixes:

- Browser IndexedDB: schema indexes were previously metadata-only. This pass
  confirmed the remaining P0 was that `queryPlanFor()` could report an index
  even when execution still used `allDocuments()` or a broad collection cursor.
  The follow-up fix now materializes schema-index entries in IndexedDB and
  aligns `queryPlanFor().strategy` with the actual execution path.
- Daemon idle loops: `consume_business_commands_loop` was confirmed as an
  idle poller and has now been moved to RxDB table-change wakeups with a long
  safety fallback. The highest remaining native idle risks are generic
  business-record projection still waking for broad source stamps/orphan repair
  epochs, module-catalog tree walks, desktop-file fallback scans, and
  service-level fixed timer wakes. These are tracked as P1/P2 work below.
- DB growth/retention: the major unresolved design gap is still the absence of
  replication-horizon-safe tombstone/chunk/blob retention. Age-only tombstone
  cleanup, inline base64 chunk/blob payloads, attachment materializations, and
  WAL/freelist maintenance need an explicit policy before release-quality
  claims.

Comprehensive follow-up subagent review on 2026-06-26 rechecked this plan
against the 2026-06-24 review and the current source:

- Coverage: all confirmed 2026-06-24 findings are represented in the coverage
  appendix, but many are still open or partial. The old rollup counts were
  wrong and are corrected below.
- Native daemon: the unchanged-active-meeting backoff issue was still real in
  the review and is now fixed by the activity detector/backoff regression;
  native RxDB file-backed external-write safety drains have since been removed
  from the per-opened-collection idle path; Notes/desktop-file loops still need
  watcher/dirty-root triggering; projection writes still reopen RxDB and
  re-read metadata per row; ticket projection hydration remains N+1.
- Browser/file sharing: demand-cache sidecar eviction still uses per-collection
  fixed timers, but the under-budget path no longer scans document-access
  records; unbounded `find().exec()` on demand-loaded collections is
  underspecified; live subscriptions still full re-query; and `rxdb.file.fetch`
  remains demand-based but not stream-oriented for consumers.
- Plan specificity gaps were added for bounded `RxSubject`
  backpressure/lagged-resync semantics, native `bulk_write` current-state
  batching, LCM/status read caching, provider-adapter transcript clone
  reduction, Business Chat active-collection lifecycle and DOM/Layout guards,
  UI performance guards, and cleanup of the orphaned report module island noted
  by the original review. The native `bulk_write` batching gap and the fixed
  Business Chat 4 second idle tracker are now closed; the rest remain tracked
  below.

Final comprehensive subagent review on 2026-06-26 after the native
external-write idle patch confirmed the updated priority stack:

- The old native file-backed per-collection 60 second safety drain finding is
  no longer current. The remaining native RxDB change-detection cost is the
  central SQLite `PRAGMA data_version` watcher plus per-table notified drains,
  which need production counters but do not create N idle safety scans for N
  opened collections.
- The biggest still-plausible "after file share" daemon costs are now file/chunk
  materialization and the remaining desktop-file watcher/retention gaps: the
  index loop still wakes periodically, but maintenance/filesystem scans no
  longer hold the native RxDB write lock and unsafe-file compaction now uses an
  indexed live-core candidate query. Demand file streaming still collects chunk
  metadata before streaming, and browser consumers still need range/stream APIs
  plus batched chunk writes.
- Projection writer fanout is a P1 structural item: per-record
  `upsert_rxdb_collection_record` still opens/reads RxDB and rechecks table
  metadata; command/file/release acceptance paths can multiply that pattern.
- Browser-side remaining P1/P2 risks are non-indexed `allDocuments()` fallback,
  default-window demand `find().exec()` semantics, full live-query re-exec,
  local-write push coalescing, browser chunk uploads, and the fixed 30 second
  sidecar stat timers if they show up in a real browser idle profile.
  Single-document `upsert()`, facade/storage `bulkUpsert()`, WebRTC
  `encodedSize()` allocation, and frame ACK reassembly bookkeeping are now
  fixed and guarded by dist-level smokes.
- A local read-only perf probe on 2026-06-26 measured
  `runtime/business-os-rxdb.sqlite3` at 276,918,272 bytes, with
  `desktop_file_chunks` holding 6,404 rows and about 99.8 MB of JSON payload,
  `desktop_files` holding 37,577 rows including 32,840 tombstones, and about
  76.3 MB of freelist pages. That makes retention/compaction a release-blocking
  performance topic, not just cleanup.

Additional 2026-06-27 subagent recheck corrected two plan-status mistakes and
sharpened the file-access idle gates:

- H4 is fixed for the exact `syncTrackedMessages` N+1 finding; remaining Chat
  cost is the separate M22 DOM/layout path.
- M22 remains open, M24 is partial, L-browser-2 is partial, and L-async-1 is
  partial. These must not be counted as closed in release summaries.
- Passive installed idle after file-access warmup must require zero deltas for
  `rxdb_sqlite.external_poll_data_version_reads`,
  `rxdb_sqlite.external_poll_changed_table_reads`,
  `rxdb_sqlite.changed_documents_since_calls`,
  `rxdb_sqlite.changed_documents_since_results`, `rxdb_sqlite.bulk_write_rows`,
  and `loops.desktop_file_index.rows`.
- Normal demand-file reads must prove the canonical chunk-id lookup path was
  used. Prefix-range fallback scans are exceptional and should fail the
  file-access perf scenario unless the scenario explicitly exercises fallback.
- File Viewer large text previews now use a bounded `rxdb.file.fetch` byte
  range; full downloads remain full reads with content-hash validation.
  Remaining file consumers still need streaming/range conversion and
  peak-retained-byte gates.

## Verified As Fixed Or Strongly Reduced

These old review findings are no longer accurate as written:

- `H1/M1/M3` native RxDB query/count and storage cursor paths: strongly reduced. The SQLite
  backend now compiles simple Mango selectors into SQL with `WHERE`, sort,
  `LIMIT`, `OFFSET`, and `COUNT(*)`; compiled query and count paths can use
  read-only WAL connections. File-backed `find_documents_by_id` now also uses
  a read-only connection plus batched `WHERE id IN (...)`, and
  `get_changed_documents_since` reads checkpoints through a read-only
  connection instead of the shared writer connection. File-backed complex
  `query()` fallback reads also run on a read-only connection, so unsupported
  Mango matchers no longer wait for the shared writer mutex. The WebRTC
  `rxdb.query.fetch` handler now applies request `window.offset`/`window.limit`
  before preparing the Mango query, rejects windows above the server cap, and
  uses a blocking storage-stream hook plus bounded producer/sender channel for
  stream-capable compiled paths. On 2026-06-26, native SQLite query-fetch was
  narrowed further: unsupported SQL stream fallbacks such as `$regex` are now
  rejected as `QUERY_NOT_SUPPORTED` instead of using the Rust matcher fallback
  on the WebRTC hot path.
  Files:
  - `src/core/rxdb/src/storage/sqlite/sql.rs`
  - `src/core/rxdb/src/storage/sqlite/instance.rs`
  - `src/core/rxdb/src/plugins/replication_webrtc/query_fetch_handler.rs`

- `H2/M24` WebRTC transport status: strongly reduced. Transport status
  emissions are throttled/coalesced and now stay skinny by default; full RTC
  connection/message snapshots are only built when `includeDiagnostics` is
  requested explicitly.
  Files:
  - `src/apps/business-os/rxdb/src/webrtc-native.mjs`
  - `src/apps/business-os/rxdb/tests/transport-status-throttle-smoke.mjs`

- `M3` WebRTC query-fetch unbounded/full-scan path: fixed for native
  query-fetch. The handler now
  applies the request window before streaming and rejects windows above a
  server cap of 25 default windows. Regression tests prove a `window` with
  `offset = 10, limit = 25` streams only that slice and that over-cap windows
  emit `STREAM_LIMIT_EXCEEDED` without data chunks. The native SQLite stream
  path now rejects non-SQL-compilable Mango queries before emitting data
  chunks, so complex fallback scans cannot run inside `rxdb.query.fetch`.
  File:
  - `src/core/rxdb/src/plugins/replication_webrtc/query_fetch_handler.rs`

- `M30` SQLite fsync pressure: fixed for the checked central paths, broader
  direct-open helper hardening remains.
  `PRAGMA synchronous=NORMAL` is now set for the Business OS store and core
  persistence.
  Files:
  - `src/core/business_os/store.rs`
  - `src/core/persistence.rs`

- `M31` status path pressure: partially fixed. Durable status reads and process
  scans are cached, and the normal status path no longer triggers Business OS
  app recovery. The dedicated Business OS app recovery loop now uses a
  source-specific leased-app queue/artifact stamp before running its
  leased-queue scan.
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

- Native RxDB `bulk_write` current-state lookup: fixed for the known adapter
  hotspot. The old full-table current-state read had already been reduced to
  written IDs; it now loads those IDs through one batched `WHERE id IN (...)`
  read instead of one point lookup per written document.
  File:
  - `src/core/rxdb/src/storage/sqlite/instance.rs`
  - `src/core/rxdb/src/storage/sqlite/sql.rs`

- Hot Business OS RxDB schema indexes: improved. `business_commands`,
  `ctox_queue_tasks`, and `desktop_file_chunks` now carry schema indexes for
  the status/command/file/generation selectors used by hot native paths. The
  generated Business OS schema contract and schema-hash registry are current,
  the browser bundle was rebuilt, the direct bundle import cache-busters and
  shell app build cache tags were bumped together, and a native
  `EXPLAIN QUERY PLAN` guard proves SQLite uses `_deleted` plus hot selector
  index prefixes instead of scanning these collections.
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
  - `src/apps/business-os/app.js`
  - `src/apps/business-os/index.html`
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
  `incremental_upsert` per chunk. Native `rxdb.file.fetch` now bridges sync
  file sources through a bounded channel and async backpressure, while the
  Business OS desktop chunk source reads the local RxDB SQLite file with
  read-only direct SQL instead of blocking on async RxDB queries.
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
  repair `find(limit)` sweeps. Queue projection repair now selects active
  statuses only, and Chat tracking repair now selects top-level
  `tracking_active = true` documents instead of a broad `business_chats`
  page. Active Chat tracking command/task lookups are batched per repair pass.
  Incremental high-water repair windows remain open.
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
  metadata, a trigger-maintained communication projection clock, and the
  queue/chat repair stamp, so unchanged idle rounds skip support intake,
  generic collection pulls, thread relevance projection, and broad queue/chat
  repair work without hashing table-size communication message payloads.
  Files:
  - `src/core/mission/channels.rs`
  - `src/core/business_os/store.rs`
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

- Mailserver SQLite hot-path connection churn: fixed. All public SQLite store
  hot paths now use the thread-local cached store connection instead of opening
  a fresh SQLite connection per operation; `init()` remains the direct-open
  schema bootstrap path. Regression counters prove IMAP SELECT/FETCH/STORE and
  a broad SMTP queue, CalDAV, CardDAV, and greylist sequence stay at one SQLite
  open after setup.
  File:
  - `src/core/mailserver/src/store/sqlite.rs`

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
  handler also emits chunks through a bounded producer/sender bridge as storage
  batches are produced. Remaining native query risk is now in complex Mango
  fallback scans and broader missing query-plan guards, not post-query frame
  buffering.
- The shared writer `Arc<Mutex<Connection>>` still serializes writes and
  in-memory read fallbacks. File-backed `query()`, `find_documents_by_id`, and
  `get_changed_documents_since` no longer use the writer mutex for read
  execution, but unsupported query shapes may still perform broad read-only
  scans.
- The internal external-write poller drains file-backed changed-since reads
  through a separate read-only SQLite connection and no longer waits for the
  shared writer connection mutex. A wake drains multiple bounded batches until
  empty or a hard per-wake budget is reached; budget exhaustion self-signals
  the poller instead of waiting for the 60 second safety poll.
- `bulk_write` now avoids both full-table current-state reads and per-ID
  current-state point queries; large batches use one batched ID lookup under
  the writer transaction.
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
- `sync_local_markdown_notes` still uses a polling loop, but it no longer keeps
  a permanent 3 second idle cadence after sources are unchanged. The active
  interval is used after real changes or errors; unchanged rounds back off to
  60 seconds. Its idle source stamp is narrow: note rows are checked through
  metadata only and the query is guarded to use a covering SQLite index instead
  of reading payload bytes.
- Business-record projection is now gated. Its `business_records` source stamp
  is a single covering-index metadata query, but the loop still wakes
  periodically. The communication projection portion of the same stamp now
  reads one trigger-maintained `communication_projection_clock` row instead of
  scanning communication account/thread/message payloads.
- Desktop file indexing still wakes periodically, but unchanged source roots now
  skip recursive candidate scans, the RxDB write path, and the projection lock
  on short idle ticks. Watcher/event-driven triggering remains open; missed
  nested changes are covered by a slow fallback scan.
- Native RxDB external-write detection uses read-only drain reads and bounded
  catch-up. File-backed storage instances now wait only for table-change
  notifications from the SQLite file watcher/trigger path after startup, so
  the old 60 second per-collection safety drain no longer scales idle work with
  the number of opened Business OS collections. The remaining architecture work
  is to keep hard counters around the DB-wide watcher and move toward a fully
  central dispatcher/backpressure design.
- Runtime settings now skip unchanged projection rounds and ignore unrelated
  core DB writes. Queue health and harness-flow inputs are still TTL-covered
  rather than separately source-stamped, so this path is reduced but not yet
  fully event driven.
- Module catalog idle projection is now source-stamped, but release projection
  and upgrade/release paths still need keyed lookups and event-driven
  reconciliation.
- Channel sync still has polling semantics, but repeated no-change adapter work
  is reduced. Configured IMAP no longer does repeated full mailbox UID scans
  after the first persisted UID, and the service-level channel scheduler now
  backs off adapters whose last sync returned no activity. Meeting session sync
  now keeps per-session file stamps, skips unchanged session JSON parsing, and
  stops counting already-known chat `message_key`s as newly ingested activity,
  active unchanged sessions are now classified as no-activity by the
  service-level due gate when `ingested = 0` and all active sessions were
  skipped unchanged.
  Channel sync is still not yet an event/IDLE/token-driven model.
- Business OS app recovery, harness audit, and idle durable-queue empty probes now have
  source-stamp gates. The durable-queue dispatcher no longer retries a
  known-empty queue on every short idle tick, while a newly persisted queue
  task reopens the gate immediately. Harness audit now watches only
  process/core-transition/process-mining inputs plus active findings, and
  ignores unrelated Core-DB writes and its own audit-run history table.

### Browser RxDB, WebRTC, And IndexedDB

- Browser IndexedDB `queryDocuments()` now handles primary-key equality and
  `$in` through bounded `findDocumentsById` candidates, schema-index
  equality/range/sort shapes through a generic IndexedDB `multiEntry` cursor,
  and browser `count()` delegates to `countDocuments()` instead of
  materializing `find().exec()`. Non-indexed selectors can still fall back to
  broad cursor/materialization paths.
- Advanced Status and similar UI count paths still need representative browser
  perf spies, but schema-indexed sorted/range list queries no longer rely on
  `allDocuments()` fallback.
- Browser file/chunk consumers no longer perform broad reads in Universal
  Importer or CV Print Builder; both use `rxdb.file.fetch` demand loading or
  keyed canonical chunk lookup.
- Demand-cache invalidation now uses a reverse `docId -> windowKey` sidecar
  index and invalidates once per remote-write batch.
- Browser local-write push events are now debounced/coalesced before running
  `pushToRemotePeers()`. CTOX-origin-only replication writes no longer trigger
  a push scan, and chunk-sized push batches no longer inherit the fixed
  500-entry scan floor.
- Demand-cache sidecar eviction now has an under-budget no-scan path. Remaining
  risk is fixed per-collection idle timers over many demand-loaded collections;
  shared/write-triggered scheduling remains open.
- Demand-loaded unbounded queries still need explicit pagination semantics so
  callers cannot accidentally get a partial 200-row window or a broad read.
- `rxdb.file.fetch` avoids broad chunk collection reads, but browser consumers
  still need streaming/range APIs to avoid full-file materialization.
- Transport status is skinny by default and sync-layer diagnostic snapshots are
  coalesced; full transport snapshots require explicit diagnostic requests.
- WebRTC `encodedSize()` no longer allocates `TextEncoder` buffers per frame,
  and incoming frame ACK bookkeeping now advances a stored contiguous sequence
  instead of rescanning received chunks for every frame.
- `collection.$` and primary-key `findOne().$` now apply changed-ID deltas
  without full re-query. Complex query subscriptions still tend toward full
  re-query/re-render patterns.

### Projection Writers And DB Growth

- Standalone `upsert_rxdb_collection_record` calls still open the RxDB DB per
  call, but Phase 4 repair/fanout paths now have a cached writer that reuses
  one connection and one metadata load per collection within the projection
  pass.
- `push_collection_records` no longer opens the Business OS core store per
  non-command document in one incoming batch, but it still needs transaction
  batching and statement/open counters before this item is release-clean.
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
  count, flags, or STORE. Configured IMAP sync uses UID watermarks and channel
  sync has no-change backoff, but IMAP IDLE, provider delta tokens/UIDVALIDITY,
  and remaining body-on-demand/projection split work remain.
- Business OS UI: chat tracked message sync, schedulers, layout reads, module
  searches, and spreadsheet recalculation still have confirmed hot paths.
- Inference: graph/arena/token host-side overhead remains mostly open.
- Mission/report: DB reopen and N+1 hydration paths remain mostly open.

## Status Matrix For 2026-06-24 Findings

| Finding | Current status | Notes |
| --- | --- | --- |
| H1 native RxDB non-PK full scans | Partial | Simple selectors/count/query-fetch compile to SQL; browser schema-index cursor plans exist; broader native/browser fallback guards remain. |
| H2 WebRTC status per frame | Fixed for exact finding | Native status is skinny by default and sync-layer diagnostic snapshots are coalesced instead of emitted per collection/frame. |
| H3 IMAP FETCH/STORE full body load | Fixed for exact finding | SELECT uses COUNT and FETCH/STORE sequence resolution uses body-free summaries. Broader mail residuals remain under M19. |
| H4 chat tracked message N+1 | Fixed for exact finding | Native Chat tracking repair uses indexed top-level active-tracking metadata and batched command/task lookups. Browser `syncTrackedMessages` now batches tracked command/task reads into one query per collection, coalesces subscription-triggered sync, and only keeps command/queue subscriptions plus the 4 second fallback active while active tracked messages exist. Chat attachments now release `desktop_file_chunks` after flush. Broader Chat DOM/layout work remains under M22 and shell/layout items. |
| H5 matching per-keystroke recompute | Open | Needs Maps, cached haystacks, debounce, representative tests. |
| H6 outbound per-row pipeline recompute | Open | Needs memoized pipeline and by-company index. |
| M1 RxDB count materializes docs | Partial | Native fixed for expressible selectors; browser `count()` now delegates to `countDocuments()` instead of `find().exec()`, but non-indexed browser selector counts still cursor-scan. |
| M2 single SQLite connection mutex | Partial | File-backed query, find-by-id, changed-since, and external poller reads use read-only connections; writes and in-memory fallbacks still share the writer. |
| M3 query-fetch full scan | Fixed for native query-fetch | Request windows are capped, compiled paths stream through the bounded bridge, and non-SQL-compilable SQLite stream fallbacks are rejected as `QUERY_NOT_SUPPORTED` before data chunks. |
| M4 projection reconcilers broad scans | Partial | SQL limit helps and several loops are source-stamped. Queue repair is status-selective and Chat tracking repair is top-level `tracking_active` selective; changed-source high-water/event windows remain. |
| M5 desktop chunk prune by file_id | Fixed for exact native prune | Stale-generation cleanup now uses read-only SQLite primary-key range selection with an `EXPLAIN` guard; broader chunk retention/browser consumer work remains separate. |
| M6 chunk writes one transaction per chunk | Partial | Native eager chunk generation and stale-generation redaction now use collection bulk upsert; remaining chunk write/prune paths still need deeper batching/direct SQL. |
| M7 demand-cache full sidecar scans | Fixed | Sidecar metadata keeps reverse document-to-window refs, and WebRTC pull/master-write batches invalidate once after materializing remote writes. |
| M8 browser upsert transaction overhead | Fixed | `storage-indexeddb.upsert()` now reads once inside one readwrite transaction, writes once, returns the written document, and has a dist-level request-count smoke. |
| M9 subscriptions full find on change | Partial | `collection.$` applies change payloads to an in-memory snapshot and `findOne(primary).$` ignores unrelated changed IDs; complex query subscriptions still need targeted windows or explicit full-refresh counters. |
| M10 browser allDocuments fallback | Partial | Primary-key equality/`$in`, schema-index equality/range/sort shapes, and finite unsorted limits are bounded; non-indexed selectors and subscription re-query paths can still fall back broadly. |
| M11 inference arena overhead | Open | Needs long-lived descriptor arenas or persistent contexts where shape permits. |
| M12 inference graph rebuild overhead | Open | Needs graph/context reuse investigation for fixed decode shapes. |
| M13 streamed event clone/deserialize | Fixed for stream delta/no-op path | Direct session now inspects method/payload event kind before cloning payloads and drops high-frequency delta/no-op events before deserialization; telemetry batching and broader transcript-copy work remain. |
| M14 blocking file stream closure | Fixed for exact stream path | `file_fetch_handler.rs` now bridges sync sources through a bounded channel, runs them on a blocking worker, and performs send/backpressure asynchronously; native demand sources use direct read-only SQLite instead of async RxDB `block_on`. |
| M15 unbounded RxSubject fanout | Fixed for native RxSubject fanout | Native `RxSubject` now uses a bounded broadcast ring, records lagged items, emits storage lag markers for recoverable change streams, invalidates incremental query buffers on lag, maps storage lag to replication `RESYNC`, exposes process-wide lag totals through native peer performance/`ctox_perf_probe --assert-idle`, and has targeted slow-peer checkpoint-recovery coverage. Installed/integration slow-peer soak evidence remains. |
| M16 `stalwart_messages` mailbox index | Fixed for exact finding | Mailbox summary/count paths have the mailbox/received index and query-plan guard. |
| M17 mailserver hot-path connection reuse | Fixed | SQLiteStore hot paths now reuse the thread-local cached connection across IMAP message/mailbox methods plus SMTP queue, CalDAV, CardDAV, and greylist operations; only schema `init()` directly opens a connection. |
| M18 send-verification full body fetches | Fixed for exact finding | Verification now uses `UID SEARCH HEADER Message-ID` and header-only `BODY.PEEK[...]`. |
| M19 email sync full UID scans | Partial | Configured IMAP sync uses UID watermarks after the first import; first import, UIDVALIDITY, IDLE, and provider delta tokens remain. |
| M20 ticket work-item assignment N+1 | Partial | Self-work list/projection hydration now batches latest assignment lookup in one query; single-load and broader ticket/queue bridge paths still need batching. |
| M21 ticket projection DB reopens | Fixed for direct projection | Direct Business OS ticket projection buckets now reuse one ticket DB connection, including control bundles; non-projection ticket/queue helper audits remain separate. |
| M22 chat full message HTML rebuild | Open | Needs signatures/append-only DOM reconcile. |
| M23 window drag forced reflow | Open | Needs geometry-read/write batching behind one rAF. |
| M24 sync.js transport diagnostics fanout | Partial | `sync.js` coalesces diagnostic snapshot publication and emits immediately only for real error/lifecycle transitions, but observer/fanout counters and remaining sanitize/record cost still need release evidence. |
| M25 spreadsheet full HyperFormula rebuild | Open | Needs persistent engine and changed-cell updates. |
| M26 matching requirements full rebuild/scans | Open | Needs Maps, debounce, and DOM reconcile. |
| M27 Buchhaltung journal joins per render | Open | Needs pre-aggregated Maps and targeted reloads. |
| M28 customers search full pane re-render | Open | Needs debounced center-only render and shared summary/index. |
| M29 projection writer reopen/table_info | Partial | Repair projection paths now use a cached RxDB writer with one table metadata load per collection per pass; broader command-acceptance fanout paths and open/statement counters remain. |
| M30 synchronous=NORMAL | Partial | Business OS store and persistence set it on checked central paths; direct SQLite open helpers still need unification so new hot paths cannot omit it. |
| M31 status ps/proc scan | Partial | Cached on normal path; explicit lifecycle/shutdown scans remain by design. |

## Current Critical Blockers From Subagent Review

These are the current blockers that keep the 2026-06-24 review from being
"handled" in a release-quality sense. They must be closed or explicitly
deferred before claiming the idle/performance problem is structurally fixed.

### P0 - Idle And File-Share Burn Risks

Closed on 2026-06-26:

- Native `rxdb.query.fetch` now rejects unsupported SQLite stream fallback
  queries instead of running Rust matcher/table-scan fallback work on the
  WebRTC hot path. `$regex` and other non-SQL-compilable Mango queries emit
  `QUERY_NOT_SUPPORTED` and no data chunks.
- File-backed SQLite external-write polling now opens a separate read-only
  connection for drain reads instead of taking the shared writer connection
  mutex. The shared-lock fallback remains only for `:memory:` test storage.
- Browser demand-cache invalidation now uses a reverse sidecar
  `docId -> windowKey` index in memory and IndexedDB backends, and WebRTC
  pull/master-write batches invalidate once after materializing remote writes
  instead of scanning every query window twice around the batch.
- Universal Importer and CV Print Builder original-file paths now avoid broad
  `desktop_file_chunks.find().exec()` reads. Virtual file reads go through
  `rxdb.file.fetch`, and CV canonical chunk repair uses keyed
  `findOne(canonicalChunkId)` probes before demand materialization.
- Browser IndexedDB small unsorted `limit` queries now use a bounded collection
  cursor and stop at `skip + limit` instead of calling `allDocuments()`.
- Browser IndexedDB schema-index equality/range/sort queries now use a generic
  `multiEntry` IndexedDB cursor over materialized schema-index keys. Query
  plans report `schema-index` only when that real execution path is available,
  so unsupported operators such as `$regex` no longer claim indexed execution.
- Native WebRTC transport-status emissions now stay skinny: steady emissions
  carry counters and lightweight pool counts, while RTC connection/message
  snapshots are only built through explicit diagnostic `includeDiagnostics`.
- Sync-layer diagnostics are now observer-gated and coalesced: collection
  status bursts update the in-memory diagnostic state immediately but clone and
  publish snapshots at most once per throttle window unless a real
  error/lifecycle transition requires immediate emission.
- Follow-up on 2026-06-26 for file materialize command dispatch: the browser
  command bus no longer treats a plain `payload.file_id` as evidence of
  browser-origin `desktop_file_chunks` upload work. `ctox.file.materialize`
  now auto-starts/flushes `desktop_files` only; `desktop_file_chunks` is started
  only through explicit sync dependencies or attachment refs that actually name
  chunk storage.
- Follow-up on 2026-06-26 for CV Print Builder normal mode: the module's normal
  required/readiness and live-subscription collection set no longer includes
  `desktop_file_chunks`. Chunk sync remains explicit for PDF import and parser
  dispatch, where browser-origin chunks actually need to be pushed.

No P0 item from this section remains open. The remaining work below is still
structural and must be handled before claiming the release is idle-clean, but it
is no longer the original file-share/browser `allDocuments()` burn path.

### P1 - Remaining Structural Work

1. Native RxDB external-write idle safety drains are fixed for file-backed
   collections. Keep the new zero-drain regression as a release gate. The
   DB-wide changed-table watcher now exposes production counters that classify
   active wakeups, standby wakeups, standby entries, active resets, drain
   batches, drain rows, and budget exhaustion. Watcher ownership is now
   de-duplicated per SQLite database path; finish the remaining central
   dispatcher/backpressure design so future change-stream work cannot regress
   into per-collection idle scanning.
2. Native SQLite query pushdown is still partial for normal storage queries.
   Unsupported Mango selectors no longer silently run unbounded full-table Rust
   matcher scans. Runtime counters expose normal fallback calls, visited rows,
   indexed-candidate fallback calls, and too-broad aborts; unsupported `count()`
   fallbacks report slow mode instead of `"fast"`. Normal `query()` fallbacks
   now consume `prepared_query.queryPlan` to build SQL candidate bounds before
   the Rust matcher runs, and broad fallbacks without useful candidate bounds
   fail after a fixed scan limit. Per-collection, per-operator, and
   collection/operator fallback attribution is exposed through the native
   heartbeat and included in the default idle budgets. Finish broader SQL
   compiler coverage, a hot-query registry, and query-plan guards for every hot
   `json_extract` path.
   The startup `browser_sessions` recovery hot path was narrowed on 2026-06-27
   from a negative selector to `status == active`, and a regression test now
   proves that recovery does not increment `query_fallback_calls`.
3. Notes and desktop files still poll. Keep current source stamps as safety
   checks, but add watcher/dirty-root triggering and prove large unchanged
   roots only perform bounded metadata reads between fallback scans. Desktop
   file maintenance after sharing no longer holds the native RxDB write lock
   or scans all live `desktop_files` rows for unsafe paths, but watcher/dirty
   root triggering is still needed to make fallback scans exceptional.
4. Projection writes are partially fixed. Repair/fanout paths now reuse a
   cached RxDB writer and cached table metadata per collection per pass, with a
   regression proving five upserts perform one `PRAGMA table_info` load. Finish
   broader command-acceptance fanout threading, add open/statement counters,
   and keep the 100-upsert O(tables) gate before release.
5. Ticket projection hydration remains N+1 and connection-heavy. Batch
   assignment lookups and thread one connection through the projection helpers.
6. Browser demand-cache sidecar eviction still uses fixed per-collection idle
   timers. The under-budget path now uses cached stats and does not scan LRU
   candidates; remaining work is quota/write-triggered or centrally coalesced
   scheduling if the fixed stat checks still show up in browser idle profiles.
7. Browser demand-loaded unbounded `find().exec()` is underspecified. Either
   require explicit finite pagination for demand-loaded collections or provide
   a real paged cursor API; file pickers must query indexed windows rather than
   rely on a partial 200-row default.
8. Browser IndexedDB single-document `upsert()` and `bulkUpsert()` are now
   collapsed to batched readwrite transactions and guarded by dist-level
   storage smokes. `collection.$` and primary-key `findOne().$` now apply
   changed-ID deltas. Remaining browser storage work is chunk writes and
   complex query subscriptions that still do redundant reads/full re-queries.
9. `rxdb.file.fetch` transport is demand-based and non-blocking, but the file
   path is not end-to-end streaming. Native indexing still builds whole-file
   chunk payloads, the native demand source still gathers chunk metadata before
   streaming, browser consumers still collect full fetch chunks in places, and
   browser chunk writes are not batched. Add chunk-stream/range APIs and prove
   large file preview/import/share paths keep bounded peak retained bytes and
   O(1) or batched write transactions.
10. DB growth/retention remains P1. `business-os-rxdb.sqlite3` currently shows
   large inline chunk payloads, many `desktop_files` tombstones, and substantial
   freelist bytes. Define replication-horizon-safe physical deletes,
   reference-based chunk/blob retention, WAL/freelist maintenance thresholds,
   and an offline-browser reconnect soak before claiming file sharing is
   release-clean.
11. Business OS UI/module hot paths remain open: chat tracking, Matching,
   Outbound, Buchhaltung, Customers, CV Print Builder, Conversations, and
   Spreadsheets need the batching/memoization/debounce work listed in Phase 6.

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
    sidecar eviction scans, `findOne`-N+1, live-query full re-query,
    unbounded demand-loaded query semantics, transport-status emissions,
    file-fetch peak retained bytes, and chunk write/flush counts.

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
- Done on 2026-06-26 for native idle evidence plumbing: the native RxDB peer
  heartbeat now includes `ctox.native_peer.performance.v1` with loop counters
  for Notes, desktop file index, projection loops, Business Records, and
  Business Commands. The same snapshot includes SQLite runtime counters for
  `bulk_write`, `query`, `count`, `find_documents_by_id`,
  `changed_documents_since`, stream queries, read-only open failures, and
  writer fallbacks. `ctox_perf_probe.py` reads the heartbeat before and after
  CPU sampling and reports numeric deltas without invoking `ctox status`.
- Done on 2026-06-26 for the local idle evidence gate:
  `ctox_perf_probe.py --assert-idle` now evaluates default CPU, status-latency,
  SQLite file-growth, native loop-work, and native SQLite delta budgets; it can
  add scenario-specific `--max-heartbeat-delta GLOB=VALUE` limits and exits
  non-zero on budget failure.
- Done on 2026-06-27 for service sync runtime visibility:
  live service status now includes a `performance` snapshot with channel-sync
  and ticket-sync runtime counters. This allows status-poll load gates to fail
  on repeated no-activity sync attempts or error loops while the passive
  `--assert-idle --skip-status` path remains status-free.
- Done on 2026-06-27 for sync-run idle diagnostics:
  `ctox_perf_probe.py` now snapshots `ticket_sync_runs` and
  `communication_sync_runs` before and after the CPU sampling window in
  read-only mode, reports `sync_run_delta.numeric_deltas`, supports
  `--max-sync-run-delta`, and includes default zero-row-growth budgets for
  those sync-run tables under `--assert-idle`.
- Done on 2026-06-27 for the installed idle Gate A/B/C workflow:
  `src/tools/perf/ctox_installed_idle_gate.py` is the checked-in runner for
  the real `ctox upgrade --dev` release path. It runs the upgrade, resolves the
  installed `ctox-real` PID, stores artifacts under
  `runtime/perf/installed-idle-*`, runs passive idle without `ctox status`,
  runs status-poll load separately, and records process-mining liveness
  evidence.
- Done on 2026-06-27 for installed release/PID identity:
  the installed gate now writes `release-identity.json` with the source git
  commit/branch/status, `ctox version`, install manifest, `current` symlink
  target, current-release and shared-launcher `ctox-real` hashes, sampled
  process command/path/hash/start time, and upgrade timestamps. A real run
  fails before Gate A if the install root/current release is missing, the
  source commit cannot be recorded, installed binary hashes differ, the
  sampled process is not `ctox-real`, the process hash cannot be tied to the
  installed release, or the sampled process predates `ctox upgrade --dev`.
  Updated on 2026-06-28: release-root installs without a shared
  `/Users/.../.local/lib/ctox/bin/ctox-real` launcher are accepted when the
  sampled process hash matches the `current` release binary hash.
- Done on 2026-06-27 for SQLite statement/write-lock timing counters:
  the native RxDB SQLite runtime snapshot now exposes statement elapsed
  total/max/buckets plus writer-lock wait/held total/max/buckets. Central
  SQLite helpers time statements with an RAII guard, and
  `ctox_perf_probe.py --assert-idle` fails on idle statement execution,
  statement elapsed time, writer-lock wait time, and writer-lock held buckets.
- Done on 2026-06-27 for external status quiet proof:
  live IPC and HTTP service status requests increment daemon-side counters and
  write `runtime/service-performance.status.json` with process PID/boot
  identity without requiring another `ctox status` call.
  `ctox_perf_probe.py --assert-idle --skip-status` reads that artifact before
  and after passive CPU sampling and fails by default if
  `status_requests.total_requests`, `status_requests.ipc_status_requests`, or
  `status_requests.http_status_requests` changes. The probe also fails on a
  wrong artifact PID, boot-ID change, missing artifact, or negative counter
  delta. Gate B intentionally runs status polling and therefore disables this
  passive artifact-delta check while separately requiring the status-poll load
  to appear as daemon `status_requests.total_requests` growth.
- Done on 2026-06-27 for process/DB release-gate scope:
  `ctox_perf_probe.py` now records every `pgrep -x ctox-real` candidate,
  aggregates candidate CPU, samples the selected process group plus selected
  PID descendants, fails `--assert-idle` when extra candidates or aggregate
  scope CPU exceed budgets, discovers known CTOX SQLite files plus
  `runtime/*.sqlite3` and `runtime/*.db`, records main/WAL/SHM/journal sizes,
  and fails on positive per-component growth under the default idle budget. The
  installed gate also fails before Gate A when a real run sees extra
  `ctox-real` candidates.
  Updated on 2026-06-28: the file-growth window now starts after the probe's
  pre-sampling read-only SQLite diagnostics, so SQLite `-shm` files created by
  the probe setup are not counted as daemon idle growth.
- Done on 2026-06-27 for DB diagnostic delta gates:
  `ctox_perf_probe.py` now snapshots page/freelist, `dbstat`, RxDB collection
  row/data/tombstone metrics, and sampled desktop chunk metrics before and
  after CPU sampling when DB diagnostics are enabled. Default idle budgets and
  `--max-db-metric-delta` can fail positive growth in those metrics.
- Done on 2026-06-27 for required default metric presence:
  default heartbeat, service-status, service-performance, and sync-run metric
  patterns now fail the idle assertion when no metric matches instead of only
  warning. Older or incomplete daemon artifacts can no longer silently satisfy
  the release gate.
- Still open: broader `EXPLAIN QUERY PLAN` guards beyond the first hot Business OS set,
  chunk/change-stream soak tests, browser perf smokes, file-access scenario
  automation, installed release proof that includes the service-performance
  status artifact, and installed 10 minute post-file-share idle evidence.

Validation:

- `python3 -m py_compile src/tools/perf/ctox_perf_probe.py` passed.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --skip-db --skip-heartbeat --pretty`
  passed and emitted `assertions.enabled=false`.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --skip-db --skip-heartbeat --max-cpu-avg 0 --pretty`
  exited 1 and emitted a structured assertion failure for unavailable CPU
  average evidence.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-status --skip-db --cpu-samples 1 --cpu-interval 0 --process-name __ctox_perf_probe_no_such_process__ --pretty`
  passed and did not call status or inspect SQLite files.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-status --skip-db --cpu-samples 1 --cpu-interval 0 --process-name __ctox_perf_probe_no_such_process__ --pretty | python3 -m json.tool`
  passed on 2026-06-26 and verified the probe emits a
  `native_peer_heartbeat` section.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --max-tables 3 --max-dbstat-rows 3 --max-chunk-rows 1000 --pretty`
  passed and produced read-only DB diagnostics for the current checkout.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-status --skip-db --cpu-samples 5 --cpu-interval 1 --pretty`
  passed against the currently running `ctox-real` PID 34277.
- `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --skip-db --skip-heartbeat --max-sync-run-delta '*.ticket_sync_runs.row_count=0' --pretty | python3 -m json.tool >/dev/null`
  passed on 2026-06-27 and verifies the read-only sync-run delta path is valid
  when no row changes during the sampling window.
- A synthetic negative run on 2026-06-27 inserted one `ticket_sync_runs` row in
  a temporary SQLite DB during the CPU sampling window; the probe exited 1 with
  `sync_run_delta.numeric_deltas.core.ticket_sync_runs.row_count = 1` and an
  assertion failure for `--max-sync-run-delta '*.ticket_sync_runs.row_count=0'`.
- `python3 -m py_compile src/tools/perf/ctox_installed_idle_gate.py src/tools/perf/ctox_perf_probe.py`
  passed on 2026-06-27.
- `python3 -m py_compile src/tools/perf/ctox_installed_idle_gate.py src/tools/perf/ctox_perf_probe.py`
  passed on 2026-06-28 after the release-root identity and SQLite `-shm`
  probe-order fixes.
- `python3 src/tools/perf/ctox_perf_probe.py --root /Users/michaelwelsch/Documents/ctox.nosync --pid 54313 --assert-idle --skip-status --skip-service-performance --cpu-samples 10 --cpu-interval 1 --pretty`
  passed on 2026-06-28 with CPU avg 0.02%, p95 0.1%, max 0.1%, and
  `database_file_growth.total_growth_bytes = 0`.
- `python3 src/tools/perf/ctox_installed_idle_gate.py --root /Users/michaelwelsch/Documents/ctox.nosync --artifact-dir /tmp/ctox-installed-idle-gate-post-probe-fix-20260628T0134Z --skip-upgrade --skip-gate-b --skip-gate-c --post-upgrade-warmup-seconds 0 --gate-a-seconds 30 --cpu-interval 1`
  failed on 2026-06-28 only because the installed daemon did not expose
  `runtime/service-performance.status.json`; release identity passed, CPU avg
  was 0.023%, CPU p95 was 0.155%, CPU max was 0.3%, and database file growth
  was 0 bytes.
- Synthetic `ctox_perf_probe.py` checks passed on 2026-06-27 for:
  - current service-performance artifact with PID/boot identity and IPC/HTTP
    counters passes passive `--assert-idle --skip-status`;
  - missing service-performance artifact fails;
  - old service-performance artifact without `http_status_requests` fails;
  - negative status-request counter delta fails.
- A synthetic `ctox_installed_idle_gate.py` check passed on 2026-06-27 proving
  Gate B fails when status polling does not appear as
  `status_requests.total_requests` growth in service performance deltas.
- A synthetic `ctox_perf_probe.py` check passed on 2026-06-27 proving an
  extra `ctox-real` candidate PID is reported as an idle assertion failure.
- A synthetic `ctox_perf_probe.py` check passed on 2026-06-27 proving high
  aggregate process-scope CPU is reported as an idle assertion failure.
- A synthetic `ctox_perf_probe.py` check passed on 2026-06-27 proving growth in
  a discovered `runtime/*.db` main file fails the per-component DB growth
  budget.
- Synthetic `ctox_perf_probe.py` checks passed on 2026-06-27 proving a
  positive RxDB collection row-count delta fails `--max-db-metric-delta`, while
  the same metric gate passes when the collection is unchanged.
- `python3 src/tools/perf/ctox_installed_idle_gate.py --root /Users/michaelwelsch/Documents/ctox.nosync --artifact-dir <tmp>/artifacts --dry-run --skip-upgrade --skip-gate-c --pid 12345 --gate-a-seconds 1 --gate-b-seconds 1 --cpu-interval 1 --status-interval 1`
  passed on 2026-06-27 and verified the installed-gate runner creates the
  expected manifest, PID resolution, release identity, Gate A/Gate B command
  metadata, status assertion, and summary artifacts without running the real
  upgrade.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-timing-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_status_request_counter_writes_status_free_perf_artifact -- --nocapture`
  passed on 2026-06-27 and verifies the daemon writes the
  status-free service performance artifact with status-request counters.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-timing-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_status_reports_channel_sync_runtime_metrics -- --nocapture`
  passed on 2026-06-27 after the service-performance schema gained process
  identity and HTTP status counters.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-timing-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::sqlite_runtime_counters_report_statement_and_writer_lock_timing -- --nocapture`
  passed on 2026-06-27 and proves real SQLite work advances statement elapsed
  and writer-lock wait/held timing counters.
- `rustfmt --edition 2021 --check src/core/rxdb/src/storage/sqlite/sql.rs src/core/rxdb/src/storage/sqlite/instance.rs`
  passed.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml query_indexed_selector_pushes_filter_and_window_into_sqlite -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::query_indexed_selector_pushes_filter_and_window_into_sqlite -- --nocapture`
  passed on 2026-06-26 after adding SQLite runtime counters.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox native_peer_status_reports_fresh_heartbeat -- --nocapture`
  passed on 2026-06-26 and asserts the native peer status exposes both native
  performance and SQLite runtime-counter schemas.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance -- --nocapture`
  passed: 24 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox hot_business_os_schema_indexes_have_sqlite_query_plan_guards -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --nocapture`
  passed on 2026-06-26: 265 unit tests and 30 conformance tests, 0 failures.
- `node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs`
  passed.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26:
  49 tests, 0 failures, 0 skipped.
- `node src/apps/business-os/rxdb/tests/command-bus-projection-smoke.mjs`
  passed on 2026-06-26 after adding the materialize/file-id regression: the
  smoke now proves `ctox.file.materialize` flushes `desktop_files` and does not
  start or flush `desktop_file_chunks`.
- `node src/apps/business-os/rxdb/tests/chunk-query-demand-disabled-smoke.mjs`
  passed on 2026-06-26.
- `node --test src/apps/business-os/shared/command-bus.test.mjs` passed on
  2026-06-26: 2 tests, 0 failures.
- `node src/apps/business-os/modules/cv-print-builder/tests/cv-print-builder.test.mjs`
  passed on 2026-06-26 after adding guards that `desktop_file_chunks` is not in
  the normal required/live collection sets.
- `node --check src/apps/business-os/modules/cv-print-builder/index.js` passed.
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs` passed.

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
- Done on 2026-06-26 for Desktop File Index maintenance after file sharing:
  maintenance and bounded filesystem scan collection now run before taking the
  native RxDB write lock. Unsafe-file compaction filters through the indexed
  live `ctox-core` file candidate query before JSON deserialization, and
  deleted chunk tombstone cleanup has a dedicated partial index.
- Done on 2026-06-25 for Desktop File materialize repair: eager file fastpaths
  no longer trust stale `generation_verified_at_ms` metadata as proof that
  chunk rows still exist. Real sync/repair rounds verify deterministic chunk
  IDs and rewrite missing chunks.
- Done on 2026-06-26 for verified materialized/eager rescans: when the stored
  generation id, file size, and `generation_verified_at_ms` marker match, the
  native file index path skips the chunk completeness scan. The repair path
  still falls back to deterministic chunk verification when metadata is missing
  or stale.
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
- Done on 2026-06-25 for Queue status counts, tightened on 2026-06-27:
  `count_queue_tasks` and `list_queue_tasks` now use the trigger-maintained
  `communication_projection_clock`, keyed by normalized status set and list
  limit, instead of DB/WAL/journal file stamps. Repeated idle status checks
  reuse cached queue views across `communication_sync_runs` metadata churn,
  while message/routing changes still advance the clock and invalidate the
  cache.
- Done on 2026-06-27 for channel-router preflight/tick broad-stamp removal:
  the idle router gates now use a composite source clock over communication
  projection state, scheduled-task due summary, pending document-report
  commands, and router-relevant ticket tables instead of whole Core/Ticket DB
  file stamps. `communication_sync_runs` and unrelated Business OS store churn
  no longer reopen the router, while real queue work and pending document-report
  commands do.
- Done on 2026-06-27 for scheduler due-scan broad-stamp removal:
  `should_skip_emit_due_scan` and `mark_emit_due_scan` now use a
  `ScheduleDueGateStamp` over `scheduled_tasks` instead of the whole Core
  DB/WAL/SHM file stamp. Unrelated Core-DB writes and `scheduled_task_runs`
  history writes no longer reopen the scheduler after an empty due scan, while
  due `next_run_at` time and real `scheduled_tasks` changes still do.
- Done on 2026-06-27 for Business OS app-recovery broad-stamp removal:
  `should_skip_idle_business_os_app_recovery` and
  `mark_business_os_app_recovery_ran` now use a leased-app queue/artifact
  source stamp instead of whole Core DB file stamps. Unrelated Core-DB writes,
  Business OS runtime-store churn, and non-app queue work stay cold; leased app
  queue work, app artifact changes, and stale-lease due time reopen recovery.
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
  projected `business_records`, a trigger-maintained communication projection
  clock, and queue/chat repair state before taking the projection write lock.
  Unchanged idle rounds skip support intake, collection pulls, thread
  relevance projection, and broad queue/chat repair. The communication stamp no
  longer reads or hashes every `communication_messages` row; account/thread/
  message/routing changes advance `communication_projection_clock` through
  SQLite triggers, and the idle stamp reads one clock row.
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
  - `notes_sync_sleep_backs_off_after_unchanged_round_and_resets_on_change`.
  - `queue_chat_repair_idle_gate_skips_unchanged_sources`.
  - `communication_intake_source_stamp_uses_projection_clock`.
  - `find_queue_task_for_command_uses_business_os_command_metadata`.
  - `queue_task_caches_ignore_sync_run_metadata_churn`.
  - `queue_task_list_cache_reuses_idle_reads_until_store_changes`.
  - `queue_task_count_cache_reuses_idle_reads_until_store_changes`.
  - `channel_router_preflight_gate_ignores_metadata_churn_and_reopens_on_queue_work`.
  - `channel_router_idle_gate_ignores_business_os_store_churn`.
  - `emit_due_scan_gate_skips_idle_until_schedule_db_changes_or_due_time_arrives`.
  - `business_os_app_recovery_idle_gate_ignores_unrelated_churn_and_reopens_on_app_sources`.
  - `documents_report_completion_query_uses_partial_command_index`.
  - `business_command_idle_wait_wakes_on_rxdb_table_change`.
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
  passed: 24 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox local_markdown_notes_source_stamp_ignores_unrelated_store_churn -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox local_markdown_notes_source_stamp -- --nocapture`
  passed: 2 tests, 0 failures. This includes the covering-index query-plan
  guard for the metadata-only Notes idle stamp.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox notes_sync_sleep_backs_off_after_unchanged_round_and_resets_on_change -- --nocapture`
  passed: 1 test, 0 failures. This guards that unchanged Notes rounds back off
  from the active 3 second interval to the 60 second idle interval and that real
  changes reset the loop to active cadence.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox business_records_projection_stamp_uses_covering_metadata_index -- --nocapture`
  passed: 1 test, 0 failures. This verifies the generic Business Records
  projection source stamp stays on the covering metadata index.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox communication_intake_source_stamp_uses_projection_clock -- --nocapture`
  passed: 1 test, 0 failures. This verifies the communication-intake portion
  of the Business Records projection stamp reads the trigger-maintained
  projection clock instead of scanning `communication_messages`, while message
  metadata updates still advance the stamp.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_business_record_projections_idle_gate_skips_unchanged_source -- --nocapture`
  passed: 1 test, 0 failures after the projection-stamp query rewrite.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox business_os_app_recovery_idle_gate_ignores_unrelated_churn_and_reopens_on_app_sources -- --nocapture`
  passed on 2026-06-27: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox app_recovery -- --nocapture`
  passed on 2026-06-27: 9 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox harness_audit_idle_gate_ignores_unrelated_churn_and_reopens_on_audit_sources -- --nocapture`
  passed on 2026-06-27: 1 test, 0 failures. This verifies unrelated Core-DB
  churn and `ctox_hm_audit_runs` history do not reopen harness audit, while
  process events and active findings do.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_module_catalog_idle_gate_skips_unchanged_projection -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_module_catalog_projects_modules_and_templates -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox business_command_idle_gate_skips_when_no_pending_commands -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-command-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox business_command_idle -- --nocapture`
  passed: 2 tests, 0 failures. This includes
  `business_command_idle_wait_wakes_on_rxdb_table_change`, which proves the
  idle command consumer wakes from an RxDB table-change notification instead
  of waiting for the long safety fallback.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-command-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance -- --nocapture`
  passed: 25 tests, 0 failures. Existing warning: `split_utf8_chunks` is
  unused.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox native_peer_consumes_pending_business_command -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_chat_repair_idle_gate_skips_unchanged_sources -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_ctox_queue_task_projections -- --nocapture`
  passed: 2 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_ctox_queue_task_projections_does_not_run_global_queue_repair -- --nocapture`
  passed on 2026-06-26: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_ctox_queue_task_projections_filters_to_active_queue_statuses -- --nocapture`
  passed on 2026-06-26: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_ctox_queue_task_projections -- --nocapture`
  passed on 2026-06-26: 4 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_chat_repair_idle_gate_skips_unchanged_sources -- --nocapture`
  passed on 2026-06-26: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_business_chat_tracking_projections_fails_orphaned_messages -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_business_chat_tracking_projections -- --nocapture`
  passed on 2026-06-26: 2 tests, 0 failures. This includes the active
  tracking selector regression that seeds 600 inactive chat documents before
  one active stale tracking chat.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox native_all_schema_hashes_match_browser_contract_fixture -- --nocapture`
  passed on 2026-06-26: 1 test, 0 failures.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26:
  47 passed, 0 failed, 2 skipped because the wire daemon was not built.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --nocapture`
  passed on 2026-06-26: 266 unit tests and 30 conformance tests, 0 failures.
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
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_router -- --nocapture`
  passed on 2026-06-27: 4 tests, 0 failures. This includes the router
  source-clock regressions for sync-run metadata churn, unrelated Business OS
  store churn, real queue work, and pending document-report commands.
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
  passed: 14 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox idle_dispatcher_backoffs_after_empty_durable_queue_probe -- --nocapture`
  passed: 1 test, 0 failures. This verifies an empty durable-queue probe is
  cached across unchanged idle ticks and that a later Core-DB change reopens
  the idle dispatcher.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox durable_queue -- --nocapture`
  passed: 10 tests, 0 failures.

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
- Done on 2026-06-25 for Notes loop cadence: after an unchanged source stamp,
  the background loop now backs off from the active 3 second interval to a
  60 second idle interval. Real note/source changes and errors reset the loop
  to the active interval.
- Notes watcher/event-driven dirty-root triggering remains open; the loop still
  polls as a fallback rather than waking directly from filesystem or store
  events.

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
- Done on 2026-06-26 for command-consumer idle polling: the consumer now waits
  on the RxDB SQLite table-change notifier for the active `business_commands`
  table and only uses the idle interval as a long safety fallback. A browser or
  WebRTC write of a `pending_sync` command wakes the loop immediately; it no
  longer opens the RxDB SQLite file every 10 seconds just to rediscover no
  pending commands.
- Done on 2026-06-25 for queue/chat repair idle churn: unchanged repair sources
  skip the broad queue/chat RxDB repair sweeps.
- Done on 2026-06-26 for local queue repair fanout: local
  `reconcile_ctox_queue_task_projections` no longer calls the global
  `store::repair_queue_projections` maintenance repair after repairing one or
  more RxDB queue projection documents. A regression seeds an unrelated old
  orphaned `ctox_queue_tasks` business record and proves local reconcile leaves
  it untouched instead of running global orphan repair.
- Done on 2026-06-26 for queue reconcile candidate narrowing:
  `reconcile_ctox_queue_task_projections` now queries only active
  `ctox_queue_tasks` statuses (`queued`, `running`, `accepted`) instead of a
  broad first-page `find(limit=500)`. A regression seeds 600 terminal queue
  documents before one active stale projection and proves the active projection
  is still selected and repaired.
- Done on 2026-06-26 for Chat tracking candidate narrowing:
  `business_chats` now carry top-level tracking metadata
  (`tracking_active`, `tracking_status`, tracking ids) with schema indexes.
  Browser chat persistence, native chat writeback, and native repair all
  maintain those fields. `reconcile_business_chat_tracking_projections` now
  selects only `tracking_active = true` chat documents instead of a broad
  first-page `find(limit=200)`. A regression seeds 600 inactive chat documents
  before one active stale chat and proves the active chat is still selected,
  repaired, and cleared from future active repair rounds.
- Done on 2026-06-26 for Chat tracking lookup batching:
  `reconcile_business_chat_tracking_projections` collects active `commandId`
  and `taskId` references before repairing messages, then loads
  `business_commands` and `ctox_queue_tasks` through two batched
  `find_documents_by_id` calls. A regression proves 40 active tracked messages
  do not run per-message projection lookups.
- Done on 2026-06-25 for the normal `command_id -> task_id` lookup:
  `find_queue_task_for_command` uses a partial SQLite expression index over
  queue message metadata before falling back to legacy prompt scanning.
- Done on 2026-06-25 for channel queue counts, tightened on 2026-06-27:
  repeated `channels::count_queue_tasks` and `channels::list_queue_tasks` calls
  are cached per normalized status set and now use `communication_projection_clock`
  instead of DB/WAL/journal file stamps, so sync-run metadata writes do not
  invalidate the cache.
- Done on 2026-06-25 for documents report command completion: the open
  `business_commands` lookup now uses a partial SQLite index, and an
  `EXPLAIN QUERY PLAN` test guards against regressing to a table scan.
- Done on 2026-06-25 for durable queue empty probes, tightened on 2026-06-27:
  the strict-idle queue dispatcher records a communication queue source stamp
  for an empty lease probe instead of the Core-DB file/WAL/SHM stamp. Repeated
  unchanged idle ticks skip the empty durable-queue read for the idle safety
  window; `communication_sync_runs` metadata churn no longer reopens the probe,
  while a newly persisted queue task changes the source stamp and reopens
  dispatch immediately.
- Done on 2026-06-27 for channel-router preflight/tick gates: the service
  router no longer keys idle preflight/tick backoff on whole Core/Ticket DB file
  stamps. It uses a composite router source stamp, so sync-run metadata writes
  and unrelated Business OS store churn stay cold, while real queue work,
  document-report command rows, ticket router sources, and due scheduled-task
  time can reopen the router.
- Still open: true queue/chat high-water/event-driven windows, non-channel
  status/count caches, any remaining unindexed command-completion scans, and
  removing the legacy prompt fallback after old queue entries age out. The
  global queue repair function remains available as an explicit maintenance
  repair path; it is no longer part of the local changed-document reconcile hot
  path.

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
- Done on 2026-06-25 for service-level channel sync backoff:
  `sync_configured_channels` now tracks a `next_due` per adapter and settings
  snapshot. Email/Jami/Meeting/Teams/WhatsApp adapters that return no activity
  are not invoked again on every 60 second service tick; repeated no-change
  outcomes back off up to 15 minutes, while activity, errors, or settings
  changes reset the due gate.
- Done on 2026-06-25 for Meeting active-session sync churn:
  `meeting_native::sync` no longer falls back to scanning `.` when a session
  directory read fails, caches each session JSON file by length/mtime stamp, and
  returns unchanged active sessions without reparsing transcript/chat payloads.
  It also checks `communication_messages.message_key` before upsert, so known
  chat lines no longer cause duplicate DB writes or false `ingested` activity.
- Done on 2026-06-26 for Meeting service due-gate backoff:
  `channel_sync_result_has_activity` no longer treats `active_sessions > 0` as
  activity when all active sessions are reported in
  `skipped_unchanged_sessions` and no messages were ingested/stored. Active
  meetings with new or changed session files still reset the backoff, but an
  active unchanged session now increments the no-activity backoff.
- Done on 2026-06-27 for unchanged channel pairing backoff:
  `channel_sync_result_has_activity` no longer treats any non-null `pairing`
  value as activity. Static WhatsApp-style `pairing_required` / `qr` artifacts
  now behave like unchanged channel state and extend the no-activity backoff;
  explicit change markers such as `changed`, `updated`, `started`,
  `qr_updated`, or non-zero update counters still reset the interval.
- Done on 2026-06-27 for channel-sync runtime visibility:
  live service status now exposes `performance.channel_sync` counters per
  adapter (`attempts`, `activity_runs`, `no_activity_runs`, `error_runs`), and
  `ctox_perf_probe.py` can compare those counters across separate status
  samples. Passive `--assert-idle --skip-status` remains status-free; status
  poll load is measured separately.
- Done on 2026-06-27 for service-level ticket-sync backoff:
  `sync_configured_tickets` now uses a per-source due gate. Successful
  configured ticket syncs with zero stored tickets, zero stored events, and zero
  resolved clarifications extend a no-activity backoff up to 15 minutes instead
  of running on every service router tick. Activity, errors, settings changes,
  or a different root reset the gate; manual ticket sync remains direct.
- Done on 2026-06-27 for progressed ticket-event idempotence:
  `upsert_ticket_event_from_adapter` now treats an identical event payload with
  an existing routing-state row as unchanged even after the route has progressed
  to `leased`, `handled`, `blocked`, or `failed`. Missing routing-state rows are
  still initialized. The translation-layer regression proves reapplying the
  same sync batch keeps `stored_event_count = 0`, appends no `ticket_sync_runs`
  row, and does not reset `route_status` or `updated_at`.
- Done on 2026-06-27 for ticket-sync runtime visibility:
  live service status now exposes `performance.ticket_sync` counters per source
  (`attempts`, `activity_runs`, `no_activity_runs`, `error_runs`) alongside the
  channel-sync counters.
- Done on 2026-06-25 for IMAP send verification body overfetch:
  `verify_imap_inbox_delivery` now searches by `UID SEARCH HEADER Message-ID`
  and fetches only `BODY.PEEK[HEADER.FIELDS (MESSAGE-ID DATE)]` for candidate
  UIDs. The polling verification loop no longer fetches full `RFC822` message
  bodies just to confirm round-trip delivery.
- Still open: provider-specific IMAP IDLE/delta tokens, UIDVALIDITY handling,
  richer header/flags pagination, body-on-demand split outside the native IMAP
  command path, and replacing channel polling with event/remote-token triggers.

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
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox imap_inbox_verification_ -- --nocapture`
  passed: 2 tests, 0 failures. This verifies the inbox verification commands
  use `UID SEARCH HEADER Message-ID` and header-only `BODY.PEEK[...]`, not full
  `RFC822` body fetches.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_due_gate -- --nocapture`
  passed: 2 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_activity_detection_covers_adapter_result_shapes -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-channel-sync-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_ -- --nocapture`
  passed on 2026-06-26: 4 tests, 0 failures. This includes
  `channel_sync_due_gate_backs_off_unchanged_active_meetings`, which proves an
  active unchanged Meeting result increments the no-activity due-gate backoff.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_due_gate -- --nocapture`
  passed on 2026-06-27: 4 tests, 0 failures. This includes
  `channel_sync_due_gate_backs_off_unchanged_pairing_payloads`, proving
  repeated identical pairing artifacts extend the no-activity backoff instead
  of resetting it to the base interval. The due-gate tests now serialize access
  to their shared in-memory gate so parallel Cargo test execution is stable.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox channel_sync_activity_detection_covers_adapter_result_shapes -- --nocapture`
  passed on 2026-06-27: 1 test, 0 failures. This covers static pairing
  artifacts as no activity and explicit pairing change markers as activity.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_status_reports_channel_sync_runtime_metrics -- --nocapture`
  passed on 2026-06-27: 1 test, 0 failures. This now asserts both
  `performance.channel_sync` and `performance.ticket_sync`.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox ticket_sync -- --nocapture`
  passed on 2026-06-27: 2 tests, 0 failures. This covers ticket-sync
  activity classification and no-activity due-gate backoff.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox canonical_sync_batch_persists_through_translation_layer -- --nocapture`
  passed on 2026-06-27: 1 test, 0 failures. This now also covers identical
  ticket events whose routing state has progressed to `leased`, `handled`,
  `blocked`, or `failed`.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox parse_service_status_accepts_missing_newer_fields -- --nocapture`
  passed on 2026-06-27: 1 test, 0 failures.
- `python3 -m py_compile src/tools/perf/ctox_perf_probe.py` passed on
  2026-06-27, along with synthetic assertion checks proving service-status
  channel-sync deltas fail over budget and passive `--assert-idle --skip-status`
  does not require status samples.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_sends_first_mention_ack_once_and_marks_priority -- --nocapture`
  passed: 1 test, 0 failures. The second unchanged sync reports
  `skipped_unchanged_sessions=1` and `ingested=0`.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_sync_ingests_active_meeting_chat -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-channel-sync-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_sync_ingests_active_meeting_chat -- --nocapture`
  passed on 2026-06-26: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox communication::meeting_native::tests::sync_ -- --nocapture`
  passed: 2 tests, 0 failures.

## Phase 2 - Finish Native RxDB SQLite Architecture

### 2.1 Full Planner Integration

Extend the current SQL compiler to consume the prepared `queryPlan` where safe:

- compound indexes;
- richer Mango selector subsets;
- schema-index matching;
- deterministic fallback to Rust matcher only after SQL narrows candidates;
- use the normalized `prepared_query.queryPlan` where it gives a safe bounded
  candidate set; Memory storage already consumes this shape, while SQLite still
  uses its local compiler for fully supported selectors and now uses
  `prepared_query.queryPlan` for fallback candidate bounds when the full
  selector is not SQL-compilable;
- cap and further narrow unsupported normal `query()` fallbacks:
  non-stream `query()` may still use a read-only matcher for compatibility, but
  planner-bounded candidates run first where possible, broad scans abort after
  the fixed fallback scan limit, fallbacks increment runtime counters for calls,
  rows visited, indexed-candidate usage, too-broad aborts, and
  collection/operator attribution, and unsupported `count()` fallbacks are
  marked slow instead of fast;
- require `EXPLAIN QUERY PLAN` guards for remaining `json_extract` hotpaths.
  Native generic blob chunk demand reads now use deterministic ID-prefix ranges
  instead of JSON-expression filtering/sorting, but browser blob readers still
  need indexed/streaming closure.

Acceptance:

- Hot selectors on `status`, `updated_at_ms`, `file_id`, `generation_id`, and
  collection-specific keys use indexed SQL plans.
- `EXPLAIN QUERY PLAN` tests prove index use for representative Business OS
  collections.
- Unsupported normal-storage fallbacks are visible through row-visit/decode
  counters and collection/operator attribution, cannot run silently on daemon
  idle or WebRTC hot paths, and cannot scan beyond the fixed broad-fallback
  limit without surfacing an error.
- Generic native `document_blob_chunks` and `spreadsheet_blob_chunks` fetches
  use guarded keyed ID-prefix plans. Browser blob reads still need indexed
  cursor/streaming closure.

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
- Done on 2026-06-25/2026-06-26 for browser delivery: the app-local RxDB
  bundle was rebuilt from `src/apps/business-os/rxdb/src/index.mjs`, and the
  direct bundle import cache-busters plus shell app build cache tags were
  bumped together. After the later schema-index cursor work the current shared
  cache tag is `20260626-eviction-idle-fastpath-v1`.
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
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26:
  49 tests, 0 failures, 0 skipped.
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
- batch `bulk_write` current-state reads with one `WHERE id IN (...)` per batch
  instead of one point lookup per document under the writer transaction;
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
- Done on 2026-06-26 for file-backed external-write polling: the poller opens
  a read-only SQLite connection for drain reads instead of locking the shared
  writer connection. A regression test holds the shared writer mutex while a
  notified external write is still emitted through the change stream.
- Done on 2026-06-26 for file-backed external-write idle safety: file-backed
  storage instances no longer use the 60 second per-collection safety drain.
  After the startup reconciliation drain, they wait on table notifications from
  the SQLite file watcher/trigger path; in-memory storage keeps the rare safety
  fallback because it cannot use a separate file-backed watcher. `close`,
  `remove`, and `Drop` now signal the table notifier so a task parked without a
  safety timer can exit promptly.
- Done on 2026-06-26 for native SQLite `bulk_write` current-state reads: the
  write transaction now loads existing documents for the written IDs with one
  batched `documents_by_ids(..., with_deleted=true)` call, preserving conflict
  detection while avoiding one `document_by_id` point lookup per written
  document.
- Done on 2026-06-27 for normal SQLite fallback narrowing: unsupported normal
  `query()` selectors now use `prepared_query.queryPlan` to build SQL candidate
  bounds before the Rust matcher runs, and fallbacks without useful candidate
  bounds abort after the fixed scan limit. Per-collection, per-operator, and
  collection/operator fallback attribution is now exposed in SQLite runtime
  counters. Remaining work is broader SQL compiler coverage and a hot-query
  registry.
- Done on 2026-06-27 for native non-stream fallback decode visibility: normal
  SQLite fallback queries now expose decoded-row counters in addition to
  visited-row counters, with collection, operator, and collection/operator row
  attribution. The default installed idle probe budgets these row/decode
  counters at zero, so a daemon-standby Rust matcher fallback cannot silently
  decode JSON documents.

Validation:

- `rustfmt --edition 2021 --check src/core/rxdb/src/storage/sqlite/instance.rs src/core/rxdb/src/storage/sqlite/sql.rs`
  passed.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml find_documents_by_id -- --nocapture`
  passed: 2 tests, 0 failures.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml changed_documents_since -- --nocapture`
  passed: 3 tests, 0 failures.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml query_fallback_does_not_wait_for_writer_mutex -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-fallback-target cargo test --manifest-path src/core/rxdb/Cargo.toml query_fallback_ -- --nocapture`
  passed on 2026-06-27: 3 unit tests, 0 failures. This verifies the normal SQLite
  fallback still avoids the writer mutex, consumes `prepared_query.queryPlan`
  to restrict an unsupported `age >= 990 AND id $regex` query to 10 indexed
  candidates instead of 1000 rows, and aborts broad fallback scans without
  candidate bounds after the fixed scan limit. It also verifies attribution by
  collection, operator family, and collection/operator pair.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-fallback-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --manifest-path src/core/rxdb/Cargo.toml query_fallback_ -- --nocapture`
  passed on 2026-06-27: verifies fallback row/decode counters, row attribution
  by collection/operator/collection-operator, writer-lock isolation, indexed
  candidate narrowing, and too-broad scan-limit accounting.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-fallback-target cargo test --manifest-path src/core/rxdb/Cargo.toml fallback -- --nocapture`
  passed on 2026-06-27: 6 unit tests and 1 conformance test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml external_write_poll_uses_read_only_connection_while_writer_mutex_is_held -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::file_backed_external_poll_has_no_per_collection_idle_safety_drains -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::bulk_write_reads_only_written_ids_state_among_many_rows -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml change_stream_ -- --nocapture`
  passed: 11 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-fallback-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance -- --nocapture`
  passed on 2026-06-27: 30 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --nocapture`
  passed on 2026-06-26: 265 unit tests and 30 conformance tests, 0 failures.

Acceptance:

- A slow fallback read does not block unrelated indexed reads or writes.

### 2.3.1 Change Stream Architecture

Reduce per-collection polling and trigger fanout:

- replace per-collection DB pollers with a central change dispatcher per SQLite
  file where possible;
- ensure changed-table reads use read-only connections;
- keep file-backed external-write safety drains out of the per-collection idle
  path; future dispatcher work must preserve the zero-drain regression for many
  idle registered collections;
- make changed-since consumers catch up until a batch is empty instead of
  relying on repeated safety polls;
- keep desktop chunk catch-up bounded even when browser demand sync has a small
  batch size.
- add source-stamp or loop-budget counters that show how many rows were
  visited/decoded per idle poll and whether a poll touched the writer mutex.
- explicitly cover the current risk that `desktop_file_chunks` change catch-up
  can advance through very small batches after a share burst.
- replace unbounded `RxSubject` subscriber channels with bounded channels plus
  explicit overflow semantics, such as "lagged, resync from checkpoint", for
  high-volume change streams.

Implementation status:

- Done on 2026-06-25 for bounded changed-batch draining: the SQLite external
  write poller now drains up to 32 bounded `changed_documents_since` batches per
  wake. If that budget is exhausted, it self-signals another immediate wake
  instead of waiting for the 60 second safety poll. Desktop file chunks keep the
  small per-batch limit, but catch-up now progresses through multiple batches
  per wake, with a dedicated `desktop_file_chunks` regression test.
- Done on 2026-06-26 for file-backed per-collection idle drains: the fixed
  safety timer is now only used for `:memory:` storage. File-backed instances
  perform one startup reconciliation, then park on table notifications; the
  regression test opens 12 collections, shortens the safety interval to 25 ms,
  waits past multiple old safety windows, and proves zero
  `changed_documents_since` calls after startup.
- Done on 2026-06-26 for native `RxSubject` fanout: subjects now use a bounded
  broadcast ring instead of per-subscriber unbounded `mpsc` queues, record
  skipped ring entries, and support explicit lag signals. SQLite and memory
  storage change streams emit a lag marker; query change-event buffers treat it
  as an invalidation that forces full re-execution, and storage-backed master
  change streams map it to replication `RESYNC`.
- Done on 2026-06-27 for DB-wide watcher production counters: the SQLite
  runtime snapshot now separates total external-poll wakeups into active and
  standby wakeups, and records standby-entry plus active-reset transitions. The
  default installed idle probe budgets these counters at zero, so a recurring
  standby safety wake or unexpected active reset is visible in passive daemon
  evidence instead of being inferred from downstream statement counters.
- Done on 2026-06-27 for DB-wide watcher drain-budget counters: the SQLite
  runtime snapshot now records external-poll drain calls, non-empty batches,
  empty terminator batches, rows visited/decoded, max rows/batches per drain,
  and budget exhaustion, with per-table row/batch/exhaustion attribution. The
  default installed idle probe budgets these counters at zero so a standby
  drain after the system should be idle fails the probe directly.
- Done on 2026-06-27 for DB-wide watcher ownership: `RxStorageSqlite`
  factories for the same SQLite path now share one external database poll
  registration with refcounted shutdown. This prevents reopened databases or
  parallel storage factories from starting duplicate file-level watcher
  threads for the same Core DB.
- Still open: remaining central dispatcher/backpressure design. The dispatcher
  should remain centralized around SQLite file-level change detection, should
  retain bounded catch-up semantics under file-share bursts, and must not
  re-add per-collection idle scans.

Validation:

- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::change_stream_drains_multiple_external_batches_per_wake -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-poll-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --manifest-path src/core/rxdb/Cargo.toml change_stream_drains_multiple_external_batches_per_wake -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-poll-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --manifest-path src/core/rxdb/Cargo.toml external_database_poll_registry_is_per_database_path -- --nocapture`
  passed on 2026-06-27: verifies multiple SQLite storage factories for one
  database path share one DB-wide poll registration and unregister it only
  after the last factory is dropped.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::change_stream_drains_ -- --nocapture`
  passed: 2 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::file_backed_external_poll_has_no_per_collection_idle_safety_drains -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::change_stream -- --nocapture`
  passed: 4 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::external_write_poll_uses_read_only_connection_while_writer_mutex_is_held -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance -- --nocapture`
  passed: 24 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml rxjs_compat::tests -- --nocapture`
  passed on 2026-06-26: 8 tests, 0 failures, including process-wide
  `RxSubject` lag counters.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml change_event_buffer::tests::lagged_marker_invalidates_incremental_buffer -- --nocapture`
  passed on 2026-06-26: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml replication_protocol::index_mod::tests::storage_master_change_stream_lag_maps_to_resync -- --nocapture`
  passed on 2026-06-26: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml replication_protocol::index_mod::tests -- --nocapture`
  passed on 2026-06-26: 4 tests, 0 failures, including slow master-change
  peer checkpoint recovery after lag.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --test-threads=1 --nocapture`
  passed on 2026-06-26: 271 unit tests and 30 conformance tests.
- `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`
  passed on 2026-06-26.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-poll-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --manifest-path src/core/rxdb/Cargo.toml external_database_poll_records_backoff_transitions -- --nocapture`
  passed on 2026-06-27.
- `node src/apps/business-os/rxdb/tests/run-all.mjs`
  passed on 2026-06-26 after the native `RxSubject` change: 47 passed,
  0 failed, 2 skipped because the wire daemon was not built.

Acceptance:

- Many registered collections do not create proportional idle polling overhead.
- A large external write batch drains deterministically without permanent
  safety-poll CPU.
- The dispatcher does not hold the writer mutex for notified file-backed drain
  reads.

### 2.4 Desktop Chunk Writes And Prune

Batch and bound chunk work:

- write all chunks of one file generation in one bulk operation/transaction;
- prune by deterministic IDs, PK prefix/range, or bounded direct SQL;
- avoid per-chunk tombstone/redaction loops where a batch operation is possible.
- keep materialize repair verifying deterministic chunk IDs when chunks may
  have been deleted outside the normal writer path.
- move large file/chunk payloads toward a content-addressed runtime blob store:
  RxDB should persist manifests, refs, hashes, sizes, and retention metadata;
  only small payloads should remain inline in SQLite JSON.
- keep direct `desktop_file_chunks.find().exec()` consumers out of browser
  modules before treating demand-only chunk sync as complete.
- enforce demand-only bridge ownership centrally: direct `startCollection()`
  calls for large chunk collections must fail unless a scoped lease owns the
  collection start.

Acceptance:

- A K-chunk file upload causes O(1) transactions, not O(K).
- Chunk cleanup never scans the whole `desktop_file_chunks` collection.
- Repeated materialization of large files has bounded SQLite growth; live bytes,
  tombstone bytes, stale generations, and freelist/WAL deltas are reportable.
- Large chunk collections cannot be accidentally activated by module startup,
  Advanced Status, or future direct sync calls without an explicit scoped lease.

Implementation status:

- Done on 2026-06-27 for central browser sync ownership: `startModule()` skips
  `desktop_file_chunks`, `document_blob_chunks`, and
  `spreadsheet_blob_chunks`; `startCollection()` now rejects direct starts for
  those collections with `DEMAND_ONLY_COLLECTION_REQUIRES_LEASE`; and
  `leaseCollection()` remains the only production path that can activate and
  later release those bridges. Guarded by
  `node src/apps/business-os/rxdb/tests/module-demand-only-collections-smoke.mjs`.
- Done on 2026-06-25 for native eager chunk generation writes: the desktop file
  sync path builds all chunk documents for a new generation in memory and writes
  them with one collection `bulk_upsert` call instead of one
  `incremental_upsert` per chunk.
- Done on 2026-06-25 for stale-generation redaction writes: prune now prepares
  all stale chunk tombstones and writes them through one collection
  `bulk_upsert` call instead of one incremental write per stale chunk.
- Done on 2026-06-25 for native stale-generation prune selection: cleanup now
  reads candidate chunk rows through a read-only SQLite query bounded by the
  deterministic primary-key range for `{file_id}_{generation_id}_{idx}` chunk
  IDs instead of issuing a Mango `file_id` query against the collection. A
  dedicated `EXPLAIN QUERY PLAN` guard seeds a large chunk table and asserts
  the cleanup query uses `SEARCH ... id>?` rather than scanning the chunk table.
- Done on 2026-06-26 for browser broad chunk consumers: Universal Importer
  virtual-file reads and CV Print Builder original-PDF reads now use
  `rxdb.file.fetch`; CV canonical chunk repair uses keyed canonical
  `findOne()` probes and demand materialization instead of broad
  `desktop_file_chunks.find().exec()` reads.
- Done on 2026-06-26 for materialized/eager file verification order: matching
  `generation_verified_at_ms` metadata is checked before the expensive chunk
  completeness scan, so normal verified rescans do not re-check every chunk id.
- Still open: large file/chunk payloads must move further toward a
  content-addressed runtime blob store, browser chunk upload/write bursts need
  bulk behavior, and DB-size/WAL/freelist maintenance still needs a full
  retention contract. Desktop file index maintenance remains a separate P1
  path: periodic compaction/missing-file marking must not hold the native RxDB
  write lock while scanning large live/tombstone file sets after a file-share
  burst. Native file indexing and demand fetch also need end-to-end streaming:
  indexing still builds all chunk documents for a file in memory, the demand
  source gathers chunk metadata before streaming, and generic blob chunk
  lookups still need the same query-plan guarantees as desktop chunks.

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
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_chunk_cleanup_uses_primary_key_range_plan -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox rescan_of_unchanged_workspace_is_a_no_op -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox materialized_large_file_survives_lazy_rescan -- --nocapture`
  passed: 1 test, 0 failures.
- `rustfmt --edition 2021 --check src/core/business_os/rxdb_peer.rs`
  passed on 2026-06-26 after adding the chunk-completeness counter guard.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox rescan_of_unchanged_workspace_is_a_no_op -- --nocapture`
  passed on 2026-06-26 and asserts verified unchanged rescans perform zero
  chunk completeness checks.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox materialized_large_file_survives_lazy_rescan -- --nocapture`
  passed on 2026-06-26 and asserts verified materialized lazy rescans perform
  zero chunk completeness checks.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_index_maintenance -- --nocapture`
  passed on 2026-06-26 after adding the indexed unsafe-file maintenance query.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_index -- --nocapture`
  passed on 2026-06-26 after moving maintenance/filesystem scan work out of
  the native RxDB write lock.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_business_chat_tracking_projections -- --nocapture`
  passed on 2026-06-26 after batching active Chat tracking command/task
  lookups.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-peer-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_chat_repair -- --nocapture`
  passed on 2026-06-26 and keeps the queue/chat repair idle gate covered.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml fallback -- --nocapture`
  passed on 2026-06-26 after adding normal query fallback counters and slow
  count fallback mode.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --nocapture`
  passed on 2026-06-26: 267 unit tests and 30 conformance tests.
- `node src/apps/business-os/rxdb/tests/run-all.mjs`
  passed on 2026-06-26: 47 passed, 0 failed, 2 skipped because the wire daemon
  was not built.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file -- --nocapture`
  passed: 24 tests, 0 failures.
- Required next checks:
  `CARGO_TARGET_DIR=/tmp/ctox-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_index_maintenance_removes_internal_file_chunks -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_index_idle_gate_skips_unchanged_scan_roots -- --nocapture`,
  `CARGO_TARGET_DIR=/tmp/ctox-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox demand_file_source_streams_active_desktop_file_generation -- --nocapture`,
  and `node src/apps/business-os/rxdb/tests/cross-process-file-fetch-smoke.mjs`.

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
- treat new schema indexes as incomplete until they are backed by actual
  IndexedDB object-store index/cursor usage; declaring an index in schema is
  not sufficient browser performance evidence.
- define unbounded `find().exec()` semantics for demand-loaded collections:
  either reject/require explicit pagination or provide a paged cursor API that
  can prove complete pagination without broad reads.

Acceptance:

- Browser `count()` does not materialize documents for indexed selectors.
- Common Business OS list/filter queries do not call `allDocuments()`.
- Advanced Status counts and CTOX preview queries with small limits prove
  bounded IndexedDB cursor reads.
- File pickers over large `desktop_files` collections use indexed windows and
  complete pagination, not a hidden partial 200-row default.

Implementation status:

- Done on 2026-06-25 for primary-key browser queries: `CtoxIndexedDbCollection`
  now detects primary-key equality and `$in` selectors, including nested primary
  paths normalized by the RxDB facade, and resolves them through
  `findDocumentsById` before falling back to broad reads.
- Done on 2026-06-25 for browser `count()`: `CtoxRxCollection.count().exec()`
  now delegates to storage `countDocuments()` when available instead of
  materializing `find().exec()`. The IndexedDB implementation counts through a
  cursor without building a document array, while primary-key counts use the
  bounded ID-candidate path.
- Done on 2026-06-26 for small unsorted browser queries: finite-limit
  `queryDocuments()` calls without sort now use a bounded collection cursor and
  stop at `skip + limit` instead of materializing the full collection through
  `allDocuments()`.
- Done on 2026-06-26 for schema-index browser queries: `storage-indexeddb`
  now stores materialized schema-index keys in a generic `multiEntry`
  IndexedDB index, lazily backfills older browser rows, and executes
  prefix-equality plus one range field through that cursor. Covered sort order
  uses forward/reverse cursor direction instead of post-`allDocuments()`
  sorting. `queryPlanFor()` now reports `schema-index` only for this real
  execution strategy; unsupported operators such as `$regex`, `$nin`,
  `$contains`, and `$elemMatch` do not claim indexed execution.
- Done on 2026-06-27 for browser query-performance guards: a new browser RxDB
  smoke verifies representative `business_commands`, `ctox_queue_tasks`,
  `desktop_files`, `desktop_file_chunks`, and `business_records` list/window
  queries report schema-index execution instead of `allDocuments()` fallback.
  It also exercises strict fallback rejection before IndexedDB opens and
  ratchets app source against direct `.allDocuments()` calls and broad
  `desktop_file_chunks.find().exec()` consumers.
- Still open: `_deleted`/LWT count specialization beyond the current
  LWT-window path, demand-loaded unbounded query semantics, and collection
  subscription deltas that avoid full re-query/re-render.

Validation:

- `node src/apps/business-os/rxdb/tests/storage-index-smoke.mjs` passed.
- `node src/apps/business-os/rxdb/tests/query-api-smoke.mjs` passed.
- `node src/apps/business-os/rxdb/tests/browser-query-performance-guards-smoke.mjs`
  passed on 2026-06-27.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-27:
  50 passed, 0 failed, 2 skipped because the wire daemon was not built.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26:
  49 tests, 0 failures, 0 skipped.

### 3.2 Demand-Cache Invalidation

Replace sidecar full scans:

- maintain a reverse `docId -> Set<windowKey>` index;
- invalidate once per batch;
- remove duplicate invalidation calls around pull/master-write.
- make query sidecar eviction write/quota-triggered or centrally coalesced,
  with an under-budget fast path that uses cached size stats and does not scan
  document-access/LRU rows on idle timer wakes.
- debounce/coalesce browser local-write push triggers from `collection.observe`;
- prevent backlog-proportional scan floors in the local-write push loop.
- keep broad browser chunk consumers out of Universal Importer and CV Print
  Builder with `rxdb.file.fetch` or keyed chunk lookup guards.
- batch browser chunk/blob writes so chunk upload bursts do not create
  per-chunk read/write/push cascades.
- remove or justify the current 500-entry minimum LWT scan floor after local
  writes; large file shares must not turn one upload into a long tail of
  repeated browser `getChangedDocumentsSince()` scans.
- keep browser tests that fail if a `desktop_file_chunks.find().exec()` full
  read is used by Universal Importer, CV Print Builder, or file-integrity
  helpers.

Acceptance:

- Invalidating 100 changed docs does not scan every query window 100+ times.
- Many demand-loaded collections sitting idle do not perform periodic
  `scanDocumentAccess()` work when the sidecar is under budget.
- An 8 MB chat/CV upload produces bounded bulk writes and bounded local push
  triggers.
- Universal Importer and CV Print Builder original-file views do not perform a
  broad `desktop_file_chunks.find().exec()`.

Implementation status:

- Done on 2026-06-26 for demand-cache invalidation: the memory and IndexedDB
  sidecar backends now maintain reverse document-to-window references,
  `invalidateDocumentChange()` uses that index instead of scanning all query
  windows, and WebRTC pull/master-write batches invalidate once after remote
  writes are materialized.
- Done on 2026-06-26 for demand-cache eviction idle scans:
  `runEvictionIfOverBudget()` now uses cached `estimatedBytes` as the normal
  under-budget fast path and returns before scanning `documentAccess`/LRU rows.
  Over-budget eviction still scans once to pick candidates, and quota recovery
  uses an explicit forced recount so stale legacy stats can still be repaired.
  `query-meta-eviction-idle-smoke.mjs` wraps the backend and asserts that an
  under-budget scheduler pass performs zero `scanDocumentAccess()` calls.
- Done on 2026-06-26 for broad browser chunk consumers: Universal Importer
  uses the file demand loader for virtual Business OS files, CV Print Builder
  uses the file demand loader for original PDF display, and CV canonical chunk
  repair uses keyed canonical chunk probes before materializing missing chunks
  from `rxdb.file.fetch`.
- Done on 2026-06-25 for remote-origin push scans: the WebRTC replication
  state now ignores collection change events whose successful writes all carry
  a `ctoxReplicationOrigin` marker. Native pulls and demand-fetched master
  state therefore do not immediately trigger local push scans that only filter
  those same CTOX-origin rows out again.
- Done on 2026-06-25 for the fixed 500-entry small-batch scan floor:
  `replicationScanLimit()` no longer imposes a hard 500-row minimum. A
  chunk-sized batch of 6 now scans at most 300 entries per call, while larger
  command-style batches retain the multiplier and global cap.
- Done on 2026-06-25 for empty scan-limit continuation: when
  `getChangedDocumentsSince()` advances through excluded remote-origin rows and
  hits its scan budget without local documents, the push loop continues from
  the advanced checkpoint instead of stopping until a future write/timer.
- Done on 2026-06-26 for local-write push bursts: local collection change
  events schedule a short coalesced push timer instead of calling
  `pushToRemotePeers()` immediately for every write. A recovery smoke proves
  three local write triggers produce one push pass while the existing in-flight
  push re-run flag remains covered.
- Done on 2026-06-27 for central chunk bridge ownership and first browser
  chunk-write batches: Command Bus file-content dependencies, Business Chat
  attachments, CV Print Builder parse/flush paths, and App Store ZIP uploads
  now use scoped `leaseCollection()` ownership for `desktop_file_chunks` and
  release the lease in `finally`. Literal direct
  `startCollection('desktop_file_chunks')` calls were removed from the shared,
  module, and desktop-app browser source. CV Print Builder and App Store ZIP
  chunk uploads now use collection `bulkUpsert()` when available instead of a
  per-chunk write loop. Contract smokes cover Command Bus, Chat attachment,
  CV Print Builder, App Store ZIP upload, and demand-only module startup.
- Done on 2026-06-27 for Browser demand stream abort lifecycle: the shared
  demand transport now tracks the peer that owns each query/file collector and
  rejects both collectors on peer close; file-demand fetches now retain their
  request IDs, expose `abortAllInFlight()`, and send best-effort
  `rxdb.file.cancel` through the transport. Shared peer close, collection peer
  removal, and replication cancel paths all call the abort hooks so a peer loss
  after request acceptance cannot leave demand collectors pending indefinitely.
- Done on 2026-06-27 for the next browser chunk/blob write batching pass:
  Explorer uploads no longer use `FileReader.readAsDataURL()` and write
  `desktop_file_chunks` through one `bulkUpsert()` call; Documents and
  Spreadsheets now write `document_blob_chunks` and `spreadsheet_blob_chunks`
  through one `bulkUpsert()` call instead of per-chunk inserts.
- Done on 2026-06-27 for browser file-demand range and persistence batching:
  file-demand in-flight deduplication now keys by `fileId` plus canonical
  range, so concurrent different ranges do not share the wrong promise while
  equivalent ranges still deduplicate. Persisted file-demand chunks now write
  through one `bulkWrite()` call per fetch instead of one transaction per
  received chunk.
- Done on 2026-06-27 for file-fetch retained-byte diagnostics: the shared
  demand-loading transport now reports current `bufferedFileChunkBytes` and
  peak `maxBufferedFileChunkBytes` in addition to chunk counts. The transport
  smoke drives a successful two-chunk `rxdb.file.fetch`, proves the current
  byte count drops back to zero on completion, and keeps the peak byte count
  available for release diagnostics. The app-local RxDB bundle was rebuilt and
  the shared `db.js`/`sync.js` cache-busters were bumped together to
  `20260627-file-buffer-bytes-v1`.
- Done on 2026-06-28 for local-push changed-since scan diagnostics: the
  WebRTC replication push loop now records local push `getChangedDocumentsSince`
  calls, scanned rows, scan-limit hits, and max scanned rows in the V1.5
  status surface and Advanced Status bridge. The recovery smoke covers the
  scan-limit continuation case and proves the diagnostic counters report the
  remote-origin-only scan page plus the following local-document page. The
  app-local RxDB bundle was rebuilt and the shared `db.js`/`sync.js`
  cache-busters were bumped together to
  `20260628-local-push-scan-metrics-v1`.
- Done on 2026-06-28 for local-push scan mitigation: browser IndexedDB storage
  now stores a `pushable` replication flag and maintains a
  `collectionPushableLwtId` index. CTOX-origin-excluding changed-since reads
  use that index, so local push scans iterate local/browser-dirty rows instead
  of reading CTOX-origin rows and filtering them after the cursor step. The
  IndexedDB v3 upgrade migrates existing records' replication flags once at
  open, and the storage smoke proves two preceding CTOX-origin rows are not
  counted in the local-push `scanned` total. The app-local RxDB bundle was
  rebuilt and the shared `db.js`/`sync.js` cache-busters were bumped together
  to `20260628-pushable-lwt-index-v1`.
- Still open: broader browser chunk-stream/range APIs and any remaining
  non-central upload/import paths, blob/chunk read/range APIs, Research blob
  reads, Universal Importer/file-integrity helpers, schema-aware file-demand
  materializers/no-persist policy for incompatible non-desktop chunk
  collections, native storage-source first-chunk metrics, and release
  diagnostic export for pending query/file collector counts. Sidecar eviction
  still uses per-collection fixed timers; if idle profiles show the cached-stat
  wakeups are still visible, move it to shared/write-triggered scheduling.

Validation:

- `node src/apps/business-os/rxdb/tests/sidecar-storage-smoke.mjs` passed.
- `node src/apps/business-os/rxdb/tests/demand-loading-transport-smoke.mjs`
  passed on 2026-06-27.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-27
  after the file-fetch retained-byte diagnostics change: 50 passed, 0 failed,
  2 skipped because the wire daemon was not built.
- `node src/apps/business-os/rxdb/tests/replication-recovery-smoke.mjs`,
  `node src/apps/business-os/rxdb/tests/v1_5-status-smoke.mjs`,
  `node src/apps/business-os/rxdb/tests/status-projection-smoke.mjs`, and
  `node src/apps/business-os/rxdb/tests/advanced-status-bridge-smoke.mjs`
  passed on 2026-06-28 for the local-push changed-since diagnostics change.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-28
  after the local-push changed-since diagnostics change: 50 passed, 0 failed,
  2 skipped because the wire daemon was not built.
- `node src/apps/business-os/rxdb/tests/storage-index-smoke.mjs` passed on
  2026-06-28 for the `collectionPushableLwtId` local-push scan mitigation.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-28
  after the `collectionPushableLwtId` local-push scan mitigation: 50 passed,
  0 failed, 2 skipped because the wire daemon was not built.
- `node src/apps/business-os/rxdb/tests/demand-loader-smoke.mjs` passed.
- `node src/apps/business-os/rxdb/tests/demand-invalidation-hotpath-smoke.mjs` passed.
- `node src/apps/business-os/modules/cv-print-builder/tests/cv-print-builder.test.mjs` passed.
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs` passed.
- `node src/apps/business-os/rxdb/tests/storage-index-smoke.mjs` passed.
- `node src/apps/business-os/rxdb/tests/replication-recovery-smoke.mjs` passed.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26:
  49 tests, 0 failures, 0 skipped.
- `node src/apps/business-os/rxdb/tests/query-meta-eviction-idle-smoke.mjs`
  passed.
- `node src/apps/business-os/shared/command-bus.test.mjs`,
  `node src/apps/business-os/shared/business-chat.test.mjs`,
  `node src/apps/business-os/modules/cv-print-builder/tests/cv-print-builder.test.mjs`,
  `node src/apps/business-os/modules/app-store/app-store.test.mjs`,
  `node src/apps/business-os/rxdb/tests/module-demand-only-collections-smoke.mjs`,
  and `node src/apps/business-os/rxdb/tests/command-bus-projection-smoke.mjs`
  passed on 2026-06-27 after the scoped chunk-bridge patch.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-27:
  49 passed, 0 failed, 2 skipped (`cross-process-file-fetch-smoke.mjs` and
  `cross-process-wire-smoke.mjs`, wire daemon not built).
- `node src/apps/business-os/rxdb/tests/demand-loading-transport-smoke.mjs`,
  `node src/apps/business-os/rxdb/tests/file-demand-loader-smoke.mjs`,
  `node src/apps/business-os/rxdb/tests/replication-recovery-smoke.mjs`,
  `node src/apps/business-os/rxdb/tests/bundle-reproducible-smoke.mjs`,
  `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and
  `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-27 after
  the peer-loss demand-stream abort patch. `run-all.mjs` again reported 49
  passed, 0 failed, 2 skipped because the wire daemon was not built.
- `node src/apps/business-os/desktop-apps/explorer/explorer.test.mjs`,
  `node src/apps/business-os/modules/documents/documents.test.mjs`, and
  `node src/apps/business-os/modules/spreadsheets/spreadsheets.test.mjs`
  passed on 2026-06-27 after the Explorer/Documents/Spreadsheets bulk chunk
  write patch.
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs` passed on
  2026-06-27 after the Explorer upload contract was tightened to reject
  DataURL materialization and require the direct byte-hash/bulk-write path.
- `node src/apps/business-os/rxdb/tests/file-demand-loader-smoke.mjs`,
  `node src/apps/business-os/rxdb/tests/demand-loading-transport-smoke.mjs`,
  `node src/apps/business-os/rxdb/tests/replication-recovery-smoke.mjs`,
  `node src/apps/business-os/rxdb/tests/bundle-reproducible-smoke.mjs`,
  `node src/apps/business-os/scripts/assert-rxdb-only.mjs`, and
  `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-27 after
  the file-demand range/batched-persistence patch. `run-all.mjs` reported 49
  passed, 0 failed, 2 skipped because the wire daemon was not built.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26:
  49 tests, 0 failures, 0 skipped.

### 3.3 WebRTC Diagnostics

Finish the transport-status cleanup:

- keep counters cheap and live;
- lazily build heavy snapshots only when observers/diagnostics UI need them;
- reduce per-collection diagnostic fanout;
- Done on 2026-06-26: `encodedSize()` now computes UTF-8 byte length without
  allocating `TextEncoder` buffers and keeps byte-parity coverage for ASCII,
  multibyte, emoji, and lone-surrogate strings.
- Done on 2026-06-26: incoming frame reassembly now keeps an incremental
  `contiguousSeq` per transfer. ACK/resume bookkeeping no longer rescans the
  received chunk map from frame 0 on each chunk; final payload assembly remains
  one O(n) pass when all chunks have arrived.
- expose file-fetch chunk streams/ranges to browser consumers so large previews,
  hashing, imports, and integrity checks do not materialize the entire file in
  memory unless the caller explicitly requests a full blob.
- Done on 2026-06-26: multiplexed protocol handshakes collect per-collection
  schema/checkpoint maps with bounded parallelism instead of awaiting each
  collection serially. The concurrency cap prevents a reconnect from launching
  unbounded IndexedDB/storage reads while removing the one-await-per-collection
  fanout.
- enforce/cap `rxdb.query.fetch` request windows before query execution.
  Done on 2026-06-25.
- stream/send `rxdb.query.fetch` frames from stream-capable storage paths as
  they are produced instead of accumulating even the bounded response set
  before sending.
  Done on 2026-06-25: the query-fetch dispatcher now runs storage production
  on a blocking worker through a synchronous storage-stream hook, bridges
  wire-ready frames through a bounded channel, and sends chunks asynchronously
  with DataChannel backpressure. A regression test blocks the producer
  mid-stream and asserts the first chunk has already been sent.
  Done on 2026-06-26: native SQLite query-fetch now rejects unsupported
  SQL stream fallback queries such as `$regex` as `QUERY_NOT_SUPPORTED` before
  sending data chunks, so complex Rust matcher fallback scans cannot run on
  this WebRTC hot path.
- remove `futures::executor::block_on` and `std::thread::sleep` from WebRTC
  `file_fetch` streaming paths; use bounded sender/backpressure instead of
  blocking the runtime path.
  Done on 2026-06-25: the file-fetch dispatcher now runs sync stream sources
  on a blocking worker, bridges chunks through a bounded channel, and performs
  WebRTC sends/backpressure with async `await`. The native Business OS demand
  file source now reads desktop chunks from the local SQLite store with
  read-only direct SQL instead of calling async RxDB from the sync source.
  Done on 2026-06-26: native WebRTC transport status now emits skinny counter
  snapshots by default. RTC connection/message snapshots and full pool details
  are only built when `getTransportStatus({ includeDiagnostics: true })` is
  called explicitly; default emissions retain only counters and lightweight pool
  counts needed by liveness guards.
  Done on 2026-06-26: `sync.js` now coalesces diagnostic snapshot publication.
  Collection bursts update `syncRuntime.diagnostics` immediately, but
  `onDiagnostic(snapshotDiagnostics(...))` runs at most once per throttle
  window unless a real error/lifecycle transition needs immediate reporting.

Validation:

- `node src/apps/business-os/rxdb/tests/sync-diagnostics-throttle-smoke.mjs`
  passed.
- `node src/apps/business-os/rxdb/tests/transport-status-throttle-smoke.mjs`
  passed.
- `node src/apps/business-os/rxdb/tests/rtc-critical-pool-smoke.mjs` passed.
- `CARGO_TARGET_DIR=/Users/michaelwelsch/Documents/ctox.nosync/runtime/build/cargo-target cargo build --release --manifest-path src/core/rxdb/Cargo.toml --example v15_wire_daemon`
  passed with one existing `split_utf8_chunks` dead-code warning.
- `node src/apps/business-os/rxdb/tests/cross-process-wire-smoke.mjs` passed.
- `node src/apps/business-os/rxdb/tests/cross-process-file-fetch-smoke.mjs`
  passed.
- `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`
  passed.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml plugins::replication_webrtc::query_fetch_handler -- --nocapture`
  passed previously: 18 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml query_fetch -- --nocapture`
  passed on 2026-06-26: 21 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml query_stream -- --nocapture`
  passed on 2026-06-26: 6 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml plugins::replication_webrtc::file_fetch_handler -- --nocapture`
  passed: 5 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox demand_file_source_streams_ -- --nocapture`
  passed: 2 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-business-users-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file -- --nocapture`
  passed: 24 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --nocapture`
  passed on 2026-06-26: 265 unit tests and 30 conformance tests, 0 failures.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26:
  49 tests, 0 failures, 0 skipped.
- `node src/apps/business-os/rxdb/tests/frame-chunking-smoke.mjs` passed on
  2026-06-26 after adding allocation-free `encodedSize()` parity coverage.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26 after
  the `encodedSize()` change: 47 tests passed, 0 failed, 2 skipped because the
  wire daemon was not built in this target path.
- `node src/apps/business-os/rxdb/tests/frame-chunking-smoke.mjs` passed on
  2026-06-26 after adding incremental frame ACK bookkeeping coverage.
- `node src/apps/business-os/rxdb/tests/no-package-manager-import-smoke.mjs`
  passed on 2026-06-26 after exercising large framed transfer reassembly through
  `createCtoxWebRtcNativePeer`.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26 after
  the frame hot-path changes: 47 tests passed, 0 failed, 2 skipped because the
  wire daemon was not built in this target path.
- `node src/apps/business-os/rxdb/tests/handshake-checkpoints-parallel-smoke.mjs`
  passed on 2026-06-26.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed on 2026-06-26 after
  the checkpoint fanout change: 48 tests passed, 0 failed, 2 skipped because the
  wire daemon was not built in this target path.

Acceptance:

- Large chunk transfer produces bounded diagnostic events and bounded
  allocation.
- Large file fetch/preview/import paths have bounded peak retained bytes or are
  explicitly marked as full-materialization slow paths.
- Slow DataChannel/file-fetch consumers cannot force unbounded in-memory frame
  or file buffering.
- Slow DataChannel/query-fetch consumers cannot force unbounded bounded-window
  frame buffering before the first chunk is sent.

### 3.4 Subscription Delta Handling

Stop re-running full queries on every collection change:

- apply changed-ID deltas when possible;
- re-query only the affected window;
- debounce/coalesce subscriptions used by UI modules.
- cover Business Chat tracking, CTOX module realtime command/queue/bug reloads,
  and collection subscriptions that currently discard the change payload.
- add live-query perf smokes with large collections and a single changed
  document, proving no full collection read or hidden demand window refresh
  occurs when the changed id cannot affect the current result.

Acceptance:

- A single record change does not rebuild full browser-side collection views.

## Phase 4 - Projection Writer And Store Batching

Tasks:

1. Cache RxDB table names and column metadata for projection writes.
2. Introduce a cached/batched `RxdbProjectionWriter` or equivalent helper that
   reuses one connection and one metadata load per table within a projection
   pass.
3. Wrap `push_collection_records` batches in one transaction where semantics
   allow it.
4. Replace `pull_collection_record` "load up to 2000 and linear scan" paths
   with keyed/indexed lookup.
5. Add missing command completion indexes if the scan remains active.
6. Add keyed invoice indexes/lookups for due-date and open-cents invoice lists.
7. Keep the trigger-maintained communication projection clock guarded: large
   message stores must not get re-hashed during idle business-record
   projection checks, and message metadata/status/routing updates must still
   advance the clock.
8. Add measurement tests for `upsert_rxdb_collection_record`: 100 projection
   upserts must not reopen SQLite and run `PRAGMA table_info` per record.
9. Batch command/file/release projection fanout in the command-accept path:
   task IDs, report IDs, source-file IDs, ACL changes, and release metadata
   should share a writer session instead of invoking per-record open/read/schema
   checks.
10. Add statement/open counters around `push_collection_records`,
   `upsert_rxdb_collection_record`, and the command projection path so a large
   file/share/release command has measurable O(tables) metadata reads, not
   O(records).

Status on 2026-06-26:

- `RxdbProjectionWriterCache` and `RxdbCollectionWriter` were added in
  `src/core/business_os/store.rs`. They validate the collection name once,
  open `business-os-rxdb.sqlite3` once per collection, resolve the RxDB table
  once, and cache `PRAGMA table_info` results for the projection pass.
- `repair_module_lifecycle_projections`, direct `repair_queue_projections`
  `ctox_queue_tasks` writes, `repair_queue_projections`
  `business_commands` status updates through
  `upsert_command_projection_from_queue_status`, and
  `repair_inline_payload_artifacts` now use the cached writer path.
- `push_collection_records` now reuses one Business OS core-store connection
  and one SQLite transaction for non-command documents in one incoming batch.
- `pull_collection_record` now uses keyed single-record lookups for the
  Business OS core store, the RxDB-table fallback, and the `communication_*`
  projection collections. It no longer loads a 2,000-row window and searches
  it in-process for those paths.
- `invoices_list_due_invoices` now reads due invoice IDs through a partial
  expression index over `accounting_invoices` state, due date, open cents, and
  record ID. The Dunning run no longer performs a broad invoice payload scan
  and post-query JSON deserialization for due-invoice filtering.
- Remaining work: thread the cached writer through broader command-acceptance
  fanout paths, add explicit SQLite open/statement counters, add command-path
  batching where semantics allow, and keep a larger 100-row O(tables) guard
  for release.

Validation:

- `rustfmt --edition 2021 --check src/core/business_os/store.rs`
- `git diff --check -- src/core/business_os/store.rs docs/ctox-performance-optimization-plan-2026-06-25.md`
- `CARGO_TARGET_DIR=/tmp/ctox-store-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox rxdb_projection_writer_cache_reuses_table_metadata_for_batch -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-store-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox module_lifecycle_projection_repair -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-store-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox repair_queue_projections_ -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-store-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox push_collection_records_batches_non_command_store_writes_in_one_transaction -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-store-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox pull_collection_record_uses_keyed_rxdb_fallback_without_scan_window -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-store-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox communication_record_projection_uses_keyed_lookup -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-store-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox due_invoice_query_uses_partial_invoice_index -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-store-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox dunning_run_creates_letters_for_overdue_invoices -- --nocapture`

Acceptance:

- Projection bursts do not reopen SQLite and re-run `PRAGMA table_info` per row.
- A 100-row projection burst performs O(tables) metadata reads and O(1) DB
  opens per pass, not 100 opens and 100 `PRAGMA table_info` rounds.
- Release/upgrade projection does keyed work, not broad pull plus linear find.
- File/share/release command acceptance does not create one projection DB open,
  one existing-row read, and one schema metadata check per projected record.

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
   Generic RxDB tombstone cleanup and desktop chunk maintenance must not
   physically delete replicated state solely by wall-clock age.
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
9. Add repeated large file share/update/delete soak coverage that records
   `desktop_file_chunks`, `desktop_files`, tombstones, freelist, and WAL growth
   before and after retention/compaction, including an offline-browser reconnect
   case.

Acceptance:

- The core DB size can be explained by top tables/collections.
- File share activity cannot grow chunk/tombstone data without bound.
- Offline browser reconnect cannot lose required tombstones or referenced
  attachments after retention runs.
- Repeated file sharing cannot leave unbounded inline chunk/tombstone growth or
  unexplained freelist/WAL growth after the documented idle maintenance window.

Current measurement:

- `python3 src/tools/perf/ctox_perf_probe.py --skip-cpu --skip-status --max-tables 20 --max-dbstat-rows 20 --max-chunk-rows 200000 --pretty`
  ran on 2026-06-26.
- `runtime/business-os-rxdb.sqlite3`: 276,918,272 bytes file size, 67,607
  pages, 18,622 freelist pages (about 76.3 MB), and 4,157,112 bytes WAL.
- Largest RxDB collections in that probe:
  `desktop_file_chunks` had 6,404 rows and 99,765,582 bytes of JSON payload;
  `desktop_files` had 37,577 rows, 32,840 tombstones, and 44,493,066 bytes of
  JSON payload. This confirms that chunk retention and file tombstone retention
  are the dominant Business OS RxDB growth topics in the current local state.

## Phase 6 - Business OS UI And Module Hot Paths

Tasks:

1. Done on 2026-06-26: batch `syncTrackedMessages` lookups and debounce
   command/queue triggers. A shared JS regression seeds 40 tracked messages and
   asserts one `find()` against `business_commands`, one `find()` against
   `ctox_queue_tasks`, and zero `findOne()` fallback calls.
2. Done on 2026-06-26 for the fixed 4 second `syncTrackedMessages` polling
   interval: it is now disarmed unless active tracked messages exist, while
   command/queue subscriptions are coalesced into one in-flight sync.
3. Done on 2026-06-26: arm chat scheduler intervals only when scheduled
   messages/countdowns exist, and clear the previous global 1 second interval
   during scheduler init/cleanup.
4. Move layout reads/writes behind one `requestAnimationFrame`.
5. Add Map indexes, cached search haystacks, and debounced search inputs for:
   - Matching;
   - Outbound;
   - Buchhaltung;
   - Customers;
   - CV Print Builder;
   - Conversations;
   - Spreadsheets.
6. For spreadsheets, keep HyperFormula engines alive where possible, use
   incremental `setCellContents`, and update changed cells rather than full
   recalculation/re-render.
7. Avoid full module reloads on unrelated collection changes.
8. Gate reporter idle watchers and startup progress loops so they do not run at
   frame-rate or fixed intervals when no visible work exists.
9. Add representative UI perf guards: keystroke tests over large fixture sets,
   render-count/DOM-rebuild counters, and module subscription tests that prove
   unrelated collection changes do not call full `loadAll`/full list render.

Acceptance:

- Typing in module search fields does not trigger O(all records) recomputation
  per keypress.
- Idle browser shell has no permanent scheduler loop unless needed.
- Representative module tests fail if a single keypress or unrelated record
  change rebuilds full panes/lists.

## Phase 7 - Communication, Inference, Execution, Mission/Report

Communication:

- keep mailbox/received index guards green and finish remaining header/flags/body
  split work outside the already-fixed native IMAP command path;
- use UIDVALIDITY-aware watermarks, provider delta tokens, and IMAP IDLE where
  appropriate;
- Done on 2026-06-26: SQLiteStore hot paths reuse the thread-local cached
  connection across IMAP message/mailbox methods plus SMTP queue, CalDAV,
  CardDAV, and greylist operations; only schema `init()` directly opens a
  connection.

Execution gateway:

- Done on 2026-06-26: direct session inspects event kind before cloning or
  deserializing, and drops high-frequency stream delta/no-op events early;
- Done on 2026-06-26: direct-session `TokenCount` API cost usage is accumulated
  per turn and written through one batch transaction instead of opening the DB
  and inserting per event;
- Done on 2026-06-26: turn-loop exact tokenization preflight results are passed
  into the direct session, so the main worker turn does not run the same
  blocking local `/tokenize` HTTP preflight twice; continuity/review turns
  still use the internal preflight when no caller-provided count exists.
  Validation:
  `cargo test --bin ctox exact_prompt_preflight_reuses_precomputed_count`,
  `cargo test --bin ctox exact_prompt`, and `cargo check --bin ctox`.
- avoid provider-adapter transcript clone/parse/re-serialize work when the
  transcript is already in the internal array representation.

Inference:

- keep descriptor arenas/graph contexts alive where shape permits;
- investigate graph reuse/reserve-once for fixed decode shapes;
- move argmax/embedding work off the host hot path where appropriate.
- cache Metal PSO lookup keys and replace locked linear scans with keyed maps.

Mission/report:

- batch ticket assignment hydration;
- thread one DB connection through projection helpers;
- Done on 2026-06-26: `report/runs.rs::list_runs` no longer re-sorts rows in
  Rust after SQL already returned `ORDER BY updated_at DESC`;
- remove remaining per-row DB opens.
- Done on 2026-06-26: `queue spill-candidates` batches queue-ticket bridge
  state and distinct failure-signature counts through one core DB connection
  instead of reopening/counting per candidate;
- Done on 2026-06-26: `emit_due_steps` reuses one opened plan DB connection
  across due goals, and the due-step idle gate is keyed by plan DB path instead
  of one global slot.
- Done on 2026-06-27: `emit_due_tasks` no longer keys the scheduler idle gate
  on whole Core DB/WAL/SHM file stamps. It records a `scheduled_tasks`
  source stamp, ignores unrelated Core-DB and `scheduled_task_runs` history
  churn, and still wakes when the next scheduled due time arrives.
- Done on 2026-06-26: `list_queue_ticket_bridges` hydrates queue tasks and
  ticket self-work items in set-based batches using the already-open core DB
  connection instead of reopening channel/ticket DBs per bridge row.
- Done on 2026-06-26: `cleanup_queue_scope` and `assert_clean_queue_scope`
  batch-load scanned queue-task metadata through one read-only Core DB
  connection instead of reopening the DB per task.

Process/runtime utilities:

- Done on 2026-06-26: process-mining SQLite authorizer read-recording policy is
  captured when the authorizer is attached, so column-read callbacks no longer
  call `std::env::var`.
- Done on 2026-06-26: absolute working-hours root cache keys are canonicalized
  once per root and reused across snapshot/cache-hit calls.
- Done on 2026-06-26: live daemon status polls keep a stamp-backed durable
  status snapshot keyed by Core DB and ticket-store change stamps; unchanged
  status stores now reuse the cached queue/ticket/LCM outcome snapshot even
  after the short TTL instead of reopening SQLite on UI cadence.
- track the orphaned `report/{scoring,claims,...}` module island as cleanup so
  dead code does not keep reappearing in performance audits.

Validation:

- `CARGO_TARGET_DIR=/tmp/ctox-phase7-low-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sqlite_authorizer_skips_read_events_by_default -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-phase7-low-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox work_hours_cache_key_reuses_absolute_root_resolution -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-phase7-low-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox emit_due -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox emit_due_scan_gate_skips_idle_until_schedule_db_changes_or_due_time_arrives -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox emit_due -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-phase7-low-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox spill_ -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-phase7-low-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox queue_ticket_bridge_list_batch_hydrates_tasks_and_tickets -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-phase7-low-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox cleanup_scope_batches_metadata_reads_for_scanned_tasks -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-phase7-low-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox api_costs::tests -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-phase7-low-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox durable_status_snapshot_reuses_unchanged_store_after_ttl -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-phase7-low-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_status -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-mailserver-m17-target cargo test --manifest-path src/core/mailserver/Cargo.toml store::sqlite -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-mailserver-m17-target cargo test --manifest-path src/core/mailserver/Cargo.toml imap -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-mailserver-m17-target cargo test --manifest-path src/core/mailserver/Cargo.toml -- --nocapture`

Acceptance:

- These subsystems no longer show avoidable O(N) or per-row DB reopen work in
  targeted profiles.

## Coverage Appendix - 2026-06-24 Review

Legend: `fixed`, `partial`, `open`, `deferred`, `rejected`.

The review reports 73 confirmed findings. It lists six named high findings
(`H1`-`H6`), while the subsystem severity table counts seven high entries
because the same `H1` RxDB/SQLite root cause is shared across two subsystem
rows. The coverage table tracks the named findings plus every medium and low
finding.

### Coverage Rollup

| Severity | Rows tracked here | Fixed | Partial | Open | Deferred | Rejected/Missing |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| High named findings | 6 | 3 | 1 | 2 | 0 | 0 |
| Medium | 31 | 13 | 10 | 8 | 0 | 0 |
| Low | 35 | 19 | 6 | 10 | 0 | 0 |

### High And Medium Coverage

| ID | Status | Coverage note | Plan owner |
| --- | --- | --- | --- |
| H1 | partial | Simple/native SQL selectors, count, capped query-fetch windows, read-only reads, and native query-fetch fallback rejection are reduced; broader planner/index guards remain. | Phase 2, P0 |
| H2 | fixed | Native WebRTC status is skinny and sync-layer diagnostics are coalesced. | Phase 3.3 |
| H3 | fixed | Exact IMAP server FETCH/STORE full-body overfetch is fixed with summaries/body-on-demand. Broader mail sync work is tracked by M19. | Phase 1.5 |
| H4 | fixed for exact finding | Browser `syncTrackedMessages` now batches command/task lookups, coalesces triggers, and only keeps command/queue subscriptions plus the fallback timer active while tracked messages exist. Remaining Chat work is tracked separately as M22 and shell/layout items. | Phase 6 |
| H5 | open | Matching search/scoring still needs Maps, cached haystacks, debounce, and tests. | Phase 6 |
| H6 | open | Outbound table still needs memoized pipeline and `pipelineByCompanyId`. | Phase 6 |
| M1 | partial | Native expressible selector counts use SQL; browser non-indexed selector counts still cursor-scan. | Phase 2.1, Phase 3.1 |
| M2 | partial | Key read paths and file-backed external polling use read-only connections, file-backed per-collection idle safety drains are removed, and `bulk_write` current-state reads are batched; write serialization, in-memory fallbacks, counters, and broader dispatcher/backpressure architecture remain. | Phase 2.3 |
| M3 | fixed | Query-fetch windows are capped, the stream bridge is bounded, and non-SQL-compilable SQLite stream fallbacks now emit `QUERY_NOT_SUPPORTED` instead of scanning before the first frame. | Phase 2.1, Phase 3.3 |
| M4 | partial | Several loops are source-stamped; repair/reconcile changed-source windows still need high-water/event design. | Phase 1.1, Phase 1.4 |
| M5 | fixed | Native stale desktop chunk prune uses deterministic primary-key range selection with query-plan guard. | Phase 2.4 |
| M6 | partial | Native eager chunk generation/redaction uses bulk upserts; large payload materialization and browser chunk writes remain. | Phase 2.4, Phase 3.2 |
| M7 | fixed | Demand-cache invalidation uses reverse document-to-window refs and WebRTC remote-write batches invalidate once. | Phase 3.2 |
| M8 | fixed | Browser `storage-indexeddb.upsert()` no longer does read/write/read; `bulkUpsert` now uses one storage batch path and one IndexedDB transaction. Complex subscription deltas remain separate M9 work. | Phase 3.4 |
| M9 | partial | Collection subscriptions and primary-key `findOne().$` apply changed-ID deltas; complex query subscriptions still re-run full queries. | Phase 3.4 |
| M10 | partial | Primary-key equality/`$in`, schema-index equality/range/sort shapes, and finite unsorted limits are bounded; non-indexed selectors and subscription re-query paths remain. | Phase 3.1 |
| M11 | open | Inference descriptor arena reuse remains. | Phase 7 |
| M12 | open | Inference graph/context reuse remains. | Phase 7 |
| M13 | fixed for stream delta/no-op path | Direct session now filters high-frequency stream delta/no-op events before payload clone/deserialization and keeps consumed agent-message parsing covered by tests. | Phase 7 |
| M14 | fixed | Native file-fetch stream no longer parks runtime workers with `block_on`/`thread::sleep`. | Phase 3.3 |
| M15 | fixed for native RxSubject fanout | Native `RxSubject` uses a bounded ring with lag counters; recoverable storage change streams emit lag markers that invalidate incremental query buffers and map to replication `RESYNC`; process-wide lag totals are surfaced through native peer performance and idle-probe assertions; targeted slow-peer checkpoint-recovery coverage exists. Installed/integration slow-peer soak evidence remains. | Phase 2.3.1 |
| M16 | fixed | Mailbox/received index and summary/count query-plan guard exist. | Phase 1.5 |
| M17 | fixed | SQLiteStore hot paths now reuse the thread-local cached connection across IMAP message/mailbox methods plus SMTP queue, CalDAV, CardDAV, and greylist operations; only schema `init()` directly opens a connection. | Phase 7 |
| M18 | fixed | Send verification uses header search/header fetch instead of full `RFC822` polling. | Phase 1.5 |
| M19 | partial | UID watermarks reduce steady scans after first import; IDLE, UIDVALIDITY, delta tokens, and first-import behavior remain. | Phase 1.5 |
| M20 | partial | Self-work list/projection assignment hydration is set-based; broader ticket/queue paths remain. | Phase 7 |
| M21 | fixed for direct projection | Direct Business OS ticket projection buckets reuse one ticket DB connection, including control bundles; non-projection ticket/queue helper audits remain separate. | Phase 7 |
| M22 | open | Chat message DOM rebuild/signature work remains. | Phase 6 |
| M23 | open | Window drag geometry read/write batching remains. | Phase 6 |
| M24 | partial | Sync-layer diagnostic snapshots are coalesced, but observer/fanout counters and remaining transport-status sanitize/record cost still need release evidence. | Phase 3.3 |
| M25 | open | Spreadsheet HyperFormula lifecycle remains. | Phase 6 |
| M26 | open | Matching requirements Maps/reconcile work remains. | Phase 6 |
| M27 | open | Buchhaltung pre-aggregation and targeted reloads remain. | Phase 6 |
| M28 | open | Customers debounced center-only render/shared summaries remain. | Phase 6 |
| M29 | partial | Cached projection writer covers repair/fanout paths; broader command-acceptance fanout paths and open/statement counters remain. | Phase 4, P1 |
| M30 | fixed for checked central stores | `synchronous=NORMAL` is set in checked central stores; direct helper guard work remains. | Phase 0/1 |
| M31 | fixed for status idle path | Normal status path process scans are cached; explicit lifecycle/probe scans remain intentional. | Phase 7 |

### Low Coverage

| ID | Review finding | Status | Plan owner |
| --- | --- | --- | --- |
| L-store-1 | `push_collection_records` opens SQLite per document | partial | One incoming non-command batch now reuses one core-store connection and one transaction; command-path batching and production open/statement counters remain. |
| L-store-2 | `complete_ready_documents_report_commands` command scan | partial | Phase 1.4, Phase 4 |
| L-store-3 | `find_queue_task_for_command` substring scan | partial | Phase 1.4 |
| L-store-4 | `invoices_list_due_invoices` broad invoice scan | fixed | Dunning due-invoice lookup now uses a partial `accounting_invoices` expression index over state, due date, open cents, and record ID; a query-plan test guards index use. |
| L-store-5 | `pull_collection_record` loads 2000 docs then linear-scans | fixed | Core-store lookup was already keyed; RxDB fallback and `communication_*` projection single-record paths now use keyed primary-key lookups, with regression tests for records beyond the old 2,000-row window and communication records. |
| L-service-1 | Status poll opens fresh LCM SQLite connection | fixed for live daemon status path | Live status polls now reuse a stamp-backed durable status snapshot across unchanged Core DB and ticket stores, with regression coverage proving unchanged polls do not reload the durable snapshot or reopen LCM outcome reads after TTL. |
| L-service-2 | `count_queue_tasks` opens fresh DB on status poll | fixed | Phase 1.4 |
| L-service-3 | Business OS app recovery scan on idle status poll | fixed | Phase 1.1 |
| L-service-4 | Process-mining SQLite authorizer reads env per column | fixed | Read-recording policy is captured once in the authorizer closure at attach time; read callbacks no longer call `std::env::var`. |
| L-service-5 | `working_hours` canonicalizes cache-hit path | fixed | Absolute working-hours root canonicalization is cached per root; a regression test covers stable reuse. |
| L-rxdb-native-1 | Business command consumer status scan | fixed | Phase 1.4 |
| L-rxdb-native-2 | `bulk_write` per-id current-state point query | fixed | Phase 2.3 |
| L-browser-1 | `encodedSize()` allocates/encodes per frame | fixed | WebRTC `encodedSize()` now computes UTF-8 byte length without allocating `TextEncoder` buffers; frame-chunking smoke covers byte parity and bundle rebuild reproducibility. |
| L-browser-2 | Local writes push immediately with scan multiplier | fixed | Immediate-push bursts are debounced, and CTOX-origin-excluding changed-since reads now use the `collectionPushableLwtId` local/browser-dirty index instead of scanning remote-origin rows and filtering them afterward. |
| L-browser-3 | Chunk reassembly recomputes contiguous sequence O(n^2) | fixed | Incoming frame reassembly keeps incremental `contiguousSeq` ACK state per transfer instead of rescanning received chunks on every frame; final payload assembly is one pass at completion. |
| L-infer-1 | Metal dispatch string key + locked linear PSO lookup | open | Phase 7 |
| L-infer-2 | Host argmax over full vocab per slot | open | Phase 7 |
| L-infer-3 | CPU token embedding dequant per decode step | open | Phase 7 |
| L-exec-1 | API cost recording opens DB/inserts per TokenCount event | fixed for direct-session TokenCount path | Direct-session `TokenCount` events now accumulate API cost records in memory for the turn and flush them through `record_api_model_usage_batch` with one DB open and one transaction; regression coverage asserts multiple records use one DB open and zero-token records use none. |
| L-exec-2 | Tokenize preflight runs blocking HTTP twice | fixed | Turn-loop exact preflight counts are now reused by the direct-session main turn, and regression coverage proves a precomputed count bypasses the runtime/tokenizer path. |
| L-exec-3 | Runtime-env entry points bypass cache | partial | Phase 1.2, Phase 7 |
| L-exec-4 | Provider adapters clone/parse/re-serialize transcript | open | Phase 7 |
| L-async-1 | `collection_checkpoints_payload` sequential checkpoint awaits | partial | Checkpoint payload reads are cached/reduced, but current checkpoint collection still has a sequential-await path; keep the bounded-parallelism claim out until the code path and smoke prove it. |
| L-mission-1 | Spill-candidate scoring fresh DB/count per task | fixed | `queue spill-candidates` now batches bridge-state lookup and distinct failure-signature counts through one core DB connection, with a regression test asserting one bridge/core DB open for the candidate scan. |
| L-mission-2 | `emit_due_steps` reopens plan DB per due goal | fixed | `emit_due_steps` now reuses one plan DB connection across due goals; the due-step scan gate is keyed per plan DB path and has regression coverage for multi-goal emission plus idle-gate isolation. |
| L-mission-3 | `list_queue_ticket_bridges` reopens DBs per row | fixed | Bridge listing now uses the open core DB connection to batch-load queue tasks and ticket self-work items, with regression coverage proving no extra channel/ticket DB opens during list hydration. |
| L-mission-4 | `cleanup_queue_scope` re-queries metadata per task | fixed | `cleanup_queue_scope` and `assert_clean_queue_scope` now batch-load scanned queue-task metadata through one read-only Core DB connection, with regression coverage asserting one metadata DB open for multiple scanned tasks. |
| L-mission-5 | `list_runs` re-sorts after SQL order | fixed | `report/runs.rs::list_runs` now returns the SQL `ORDER BY updated_at DESC` result directly. |
| L-shell-1 | Chat scheduler fixed 1 s interval | fixed | Phase 6 |
| L-shell-2 | Chat scroll/resize/drag unthrottled layout work | open | Phase 6 |
| L-shell-3 | Reporter duplicate high-frequency activity listeners | open | Phase 6 |
| L-shell-4 | Startup progress 16 ms interval creep | open | Phase 6 |
| L-module-1 | CV Print Builder search rebuild/listener churn | open | Phase 6 |
| L-module-2 | Conversations reloads all messages/thread list | open | Phase 6 |
| L-module-3 | Outbound realtime subscriptions funnel into full `loadAll` | open | Phase 6 |

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
5. run `python3 src/tools/perf/ctox_installed_idle_gate.py --release` or the
   equivalent scenario-specific artifact command;
6. verify the installed `current` symlink points at the new release;
7. sample the actual installed `ctox-real` process after startup work has
   finished;
8. record idle CPU, status latency, wakeups, and DB-size evidence in this plan
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

1. Add hard measurement gates before the next release:
   remaining browser spies for `scanQueryWindows()`, sidecar eviction scans,
   live-query full re-query, heavy diagnostics snapshots, and remaining broad
   chunk-consumer/source guards.
2. Remove remaining P1 daemon idle-loop sources:
   Notes dirty flag/watcher, desktop-file watcher/dirty roots with slow
   fallback, provider-specific IMAP IDLE/delta token support, and finer service
   gates so unrelated Core-DB/WAL writes do
   not reopen router/app/harness work.
3. Finish the native RxDB/WebRTC architecture:
   complex Mango fallback reject/narrow/guard behavior, read-only or central
   dispatcher change detection without reintroducing per-collection idle
   drains, bounded `RxSubject` overflow/lagged-resync semantics, DB-wide
   watcher counters, and keeping explicit coverage that chunk catch-up after a
   file-share burst cannot create long-running safety-poll CPU.
4. Fix Browser IndexedDB/File Sharing P1s:
   explicit browser spies for unexpected non-indexed `allDocuments()` fallback,
   demand-loaded unbounded query/paged cursor semantics, Advanced Status
   representative query guards, shared/write-triggered sidecar eviction
   scheduling if cached-stat timer wakes still show up, remaining local push
   scan/fallback counters, complex subscription deltas, chunk-stream/range
   file-fetch consumers, chunk bookkeeping, and bulk browser chunk uploads.
5. Fix remaining projection-writer and DB-growth fundamentals:
   thread the cached RxDB projection writer through broader command-acceptance
   fanout helpers, add open/statement counters, finish command-path batching,
   define replication-horizon-safe tombstone/chunk retention, add attachment
   reference retention, define WAL/freelist maintenance with operator-facing
   DB-size diagnostics, and prove the current `desktop_files` tombstone /
   `desktop_file_chunks` payload footprint shrinks after a documented idle
   maintenance window without breaking offline browser reconnect.
6. Keep the remaining 2026-06-24 review work explicit:
   Business Chat active-collection lifecycle and DOM render guards,
   module/UI keystroke guards,
   remaining mailserver UID/IDLE/delta-token work, provider transcript clone reduction,
   LCM/status read caching, ticket assignment hydration, and the orphaned
   report module cleanup must not be treated as implicitly covered by the
   lower-level RxDB work.
