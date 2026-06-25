# CTOX Performance Optimization Plan - 2026-06-25

Source review: `/Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md`

This plan verifies the 2026-06-24 performance review against the current
checkout and turns it into an implementation sequence. It is intentionally
architectural: the main problem is not one bad SQLite call, but several hot
paths using generic durable stores as polling/indexing engines without pushing
filters, limits, batching, or status caching down to the right layer.

## Current Answer: Not Fully Addressed

The review has not been fully handled.

Already improved on `main`:

- `234391f1` cached several Business OS idle runtime config probes.
- `3d494fbf` stopped one idle dispatcher path from repeatedly scanning RxDB
  queue projections.
- `ce5bb23d` skips persistent backend residue scans while the service is
  running.
- The native RxDB SQLite backend already has WAL plus `synchronous=NORMAL` in
  `src/core/rxdb/src/storage/sqlite/types.rs`.
- Native RxDB `bulk_write` current-state lookup now loads only written ids
  instead of scanning the whole table.

Still open, including the dominant root cause:

- Non-primary-key RxDB SQLite queries still full-scan `SELECT data FROM <table>`
  and JSON-deserialize rows in Rust. `LIMIT` and most predicates are still not
  SQL-pushed.
- `count()` still delegates to `query()` and counts materialized documents.
- The native SQLite backend still exposes one `Arc<Mutex<Connection>>` for most
  reads and writes.
- Status IPC still performs process-table inspection on the live status path.
- WebRTC transport diagnostics no longer emit once per frame after the current
  worktree change, but cross-process wire verification and broader browser
  status/render hot paths remain open.
- Business OS browser queries and UI render paths still contain broad
  full-scan, full-rebuild, and per-keystroke recompute patterns.
- IMAP/email, execution gateway, inference, and mission/report findings remain
  mostly open.

## Verification Notes From Current Code

Representative current evidence:

- `src/core/rxdb/src/storage/sqlite/instance.rs:664` parses the Mango query and,
  when no primary-key selector is found, calls `for_each_document` at
  `instance.rs:709`.
- `src/core/rxdb/src/storage/sqlite/sql.rs:170` implements `for_each_document`
  as `SELECT data FROM <table>` with per-row `serde_json::from_str`.
- `src/core/rxdb/src/storage/sqlite/instance.rs:726` implements `count()` by
  awaiting `query()` and returning `documents.len()`.
- `src/core/rxdb/src/storage/sqlite/types.rs:36` still defines
  `SharedSqliteConnection = Arc<Mutex<Connection>>`.
- `src/core/rxdb/src/storage/sqlite/instance.rs:906` and `:947` show
  `query_stream` still walking all documents, although the limited top-K path
  bounds memory.
- `src/core/service/service.rs:3127` still calls `runtime_lifecycle_alerts()`
  during status snapshots. `service.rs:3266` now gates backend residue on
  `!service_running`, but `service.rs:3260` still calls
  `matching_service_processes()`, and `service.rs:3494` still runs `ps -axo`.
- `src/core/business_os/rxdb_peer.rs:4615` and `:4898` still run broad
  reconciliation queries with only `limit`, which does not reach SQLite for the
  generic query path.
- `src/core/business_os/rxdb_peer.rs:6453` still writes desktop file chunks via
  one `incremental_upsert` per chunk.
- `src/core/business_os/rxdb_peer.rs:6823` still prunes chunks by querying
  `file_id`, which depends on the generic non-PK query path.
- `src/core/business_os/store.rs:721` sets WAL and busy timeout, but not
  `synchronous=NORMAL`.
- `src/core/persistence.rs:197` sets WAL and busy timeout, but not
  `synchronous=NORMAL`.
- `src/apps/business-os/rxdb/src/webrtc-native.mjs:1409` builds the heavy
  transport snapshot. `:1452`, `:1492`, and `:1497` emit that snapshot on status
  updates.
- `src/apps/business-os/rxdb/src/storage-indexeddb.mjs:172` falls back to
  `allDocuments()` for most browser queries.
- `src/apps/business-os/rxdb/src/rx-database.mjs:182` implements browser
  `count()` as `find().exec().length`.
- `src/apps/business-os/shared/business-chat.js:2099` still does serial tracked
  message lookups, and `:288` polls every 4 seconds.
- `src/core/mailserver/src/imap/mod.rs:234` and `:359` still load all mailbox
  messages for FETCH/STORE.
- `src/core/mailserver/src/store/sqlite_schema.rs:83` still defines
  `stalwart_messages` without a mailbox/received index.
- `src/core/execution/agent/direct_session.rs:1582` still clones/deserializes
  every event candidate before filtering consumed event kinds.
- `src/core/inference/models/qwen35_27b_q4km_dflash/src/driver.rs:179` still
  frees/reinitializes the target graph context per target step.

## Subagent Verification Summary

Three read-only explorer agents independently checked the current code after
the review:

- Native RxDB/SQLite explorer: confirmed H1 remains only partially fixed. PK
  equality is bounded, `query_stream` has a bounded-memory top-K path, and
  `bulk_write` no longer full-scans for current state. The core non-PK adapter
  problem remains: SQL does not consume `queryPlan`, `count()` materializes, and
  most reads still use the shared connection mutex.
- Daemon/service-loop explorer: confirmed `ce5bb23d` fixes only the backend
  residue subset of M31. Running status still forks `ps -axo`, opens/reads
  durable stores, and can trigger app recovery from the status path. Service
  idle/status remains a separate open workstream.
- Browser/Business-OS explorer: confirmed H2/H4/H5/H6 and M7-M10/M22-M28 are
  mostly still open. Some paths are debounced, but heavy WebRTC diagnostics,
  full-query subscriptions, sidecar scans, serial chat lookups, and keystroke
  recomputation remain.

No subagent edited files or ran tests; this plan integrates their source
inspection findings.

## Implementation Progress - 2026-06-25

Status: started, not complete.

Implemented in the current worktree after this plan was first written:

- Native RxDB SQLite now has a conservative SQL compiler for expressible Mango
  selectors: scalar equality/range/`$in`, `_deleted`/`deleted`,
  `_meta.lwt`/`lastWriteTime`, schema-index fields through `json_extract`,
  sort, `LIMIT`, and `OFFSET`.
- Collection creation now creates SQLite expression indexes for schema index
  fields that are not backed by existing columns.
- Native `query()`, `query_stream()`, and `count()` use the SQL compiler when
  possible and fall back to the existing Rust matcher for unsupported queries.
- Compiled `query()` and `count()` reads now use a dedicated read-only SQLite
  connection when file-backed storage is available, so normal indexable reads
  do not hold the shared writer mutex.
- Compiled `query_stream()` now iterates SQLite rows directly and emits chunks
  without first materializing the complete compiled result window.
- Service status now caches the durable queue/ticket/LCM enrichment for a short
  TTL while keeping live in-memory busy/current-worker fields uncached.
- Core persistence and the central Business OS store connection now configure
  WAL with `synchronous=NORMAL`, reducing fsync pressure on the durable store
  paths called out in Phase 6.
- `status_from_shared_state()` no longer runs Business OS app recovery. Status
  polling is now a read-side control-plane operation for that path; abandoned
  Business OS app task recovery remains on the dedicated maintenance loop and
  worker-finalization/explicit recovery paths.
- Browser WebRTC transport status is now coalesced instead of emitting and
  rebuilding the full diagnostic snapshot once per frame or message metadata
  update. `getTransportStatus()` still exposes live counters, and connection
  events remain immediate.
- A focused SQLite regression test proves that an indexed `age >= 990` query
  returns a limited window from SQL, `count()` uses SQL, and
  `EXPLAIN QUERY PLAN` contains the generated expression index.
- A second regression test holds the shared SQLite connection lock and proves
  compiled `query()` and `count()` still complete through the read-only
  connection path.
- A third regression test corrupts a later raw row and proves `query_stream()`
  can stop after the first compiled SQL batch without deserializing the
  remaining rows.
- Service lifecycle status now uses a short TTL cache for duplicate-process
  detection while the daemon is already running. Stop/cleanup paths still use a
  fresh process scan.

Validated so far:

- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-perf-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::query_indexed_selector_pushes_filter_and_window_into_sqlite -- --nocapture`
  passed.
- `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml` passed after
  formatting the RxDB crate.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-perf-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance::tests::query_ -- --nocapture`
  passed: 8 targeted SQLite query/stream tests.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-perf-target cargo test --manifest-path src/core/rxdb/Cargo.toml`
  passed after the compiled streaming fix: 246 unit tests, 30 conformance
  tests, and doc tests.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox runtime_lifecycle_alerts_report -- --nocapture`
  passed after the service cache and SQLite PRAGMA changes: 2 tests, 0
  failures.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox status_snapshot -- --nocapture`
  passed after removing status-triggered app recovery: 11 tests, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox explicit_idle_recovery -- --nocapture`
  passed: 1 test, 0 failures.
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox service_status -- --nocapture`
  passed after the durable status cache and slow-socket guard update: 8 tests,
  0 failures.
- `node src/apps/business-os/rxdb/tests/transport-status-throttle-smoke.mjs`
  passed.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` passed: 44 tests passed,
  0 failed, 2 cross-process wire tests skipped because the wire daemon was not
  built.

Still open from the same root cause:

- The SQL compiler does not yet use the full `query_planner.rs` plan surface or
  compound schema indexes.
- Unsupported/fallback query paths and primary-key lookup still use the shared
  connection mutex; the read-connection split is incomplete for those paths.
- Unsupported Mango operators still fall back to full Rust scans.
- Status durable reads are now TTL-cached, but not yet fully event-driven or
  precomputed.
- Projection high-water marks, retention, cross-process wire verification, and
  broader Business OS UI hot paths remain open.

## Design Principle

CTOX is a background daemon. The default idle path must be event-driven or
bounded by cheap cached state. SQLite is the durable source of truth, not a
polling engine for every UI tick, frame, or daemon status request.

For this plan, "fixed" means:

- idle CPU for a live, non-busy `ctox-real` process stays under 2 percent over a
  5 minute clean sample with no status-poll load;
- status IPC p95 stays below 100 ms and does not fork/exec on normal polls;
- RxDB query paths prove bounded row visits for indexed selectors and limits;
- browser sync diagnostics do not allocate/broadcast per frame in steady state;
- database size is explainable by top tables, retention policy, and tombstone
  cleanup evidence.

## Phase 0 - Measurement And Guard Rails

Goal: make regressions visible before broad refactors.

Tasks:

1. Add an idle CPU measurement script under `src/scripts/` or `tests/tools/`
   that samples a target PID without issuing `ctox status`.
2. Add a status-path profiler helper that can run `sample <pid>` on macOS and
   collect `ctox status --json` latency distributions separately.
3. Add SQLite database-size diagnostics:
   - page count and freelist count;
   - top tables by bytes using `dbstat` when available;
   - top RxDB collections by row count and tombstone count;
   - largest WAL files;
   - stale chunk generations and retained desktop file chunk bytes.
4. Add query instrumentation to native RxDB tests: for indexable selector
   queries, count deserialized rows and assert it is bounded by
   `limit + small_slack`, not table size.
5. Add browser diagnostics tests that simulate many WebRTC frames and assert
   transport-status emissions are throttled.

Acceptance:

- A repeatable `make`/script command records idle CPU, status latency, and DB
  growth evidence.
- CI/test guards fail if a limited indexed query deserializes the whole table.

## Phase 1 - Native RxDB SQLite Adapter

This is the highest leverage work. It addresses H1, M1, M2, M3, M5, M6, M15,
and several low findings.

### 1.1 Query Compiler And Planner Wiring

Implement a SQLite query compiler for the safe subset of Mango/RxDB queries:

- primary-key equality and `$in`;
- `_deleted` / `deleted`;
- `_meta.lwt` / `lastWriteTime`;
- common scalar schema-index fields via `json_extract(data, '$.<field>')`;
- equality, range, sort, `LIMIT`, and `OFFSET`;
- fallback to current Rust matcher for non-expressible selectors.

Use the existing `query_planner.rs` to choose schema indexes. For collection
schema indexes, create SQLite expression indexes at collection-table creation
time. Keep a final Rust matcher as a correctness safety net when needed, but
only after SQL has reduced candidate rows.

Implementation files:

- `src/core/rxdb/src/storage/sqlite/sql.rs`
- `src/core/rxdb/src/storage/sqlite/instance.rs`
- `src/core/rxdb/src/query_planner.rs`
- focused tests in `src/core/rxdb/src/storage/sqlite/instance.rs` or
  `src/core/rxdb/tests/`

Acceptance:

- `find({ selector: { status: { $eq: ... } }, limit: 25 })` does not scan a
  full collection.
- `find({ selector: { file_id: { $eq: ... } } })` uses an indexable SQL path.
- `query_stream` and `rxdb.query.fetch` use the same compiler.
- `EXPLAIN QUERY PLAN` tests prove index use for hot selectors.

### 1.2 SQL Count

Replace `count()` materialization with SQL `COUNT(*)` for expressible selectors.
Fallback can keep the current path for selectors the compiler cannot express.

Acceptance:

- `count()` over `status`, `file_id`, `_deleted`, and `lastWriteTime` uses SQL
  count and does not deserialize documents.

### 1.3 Connection Architecture

Split SQLite access:

- one writer connection protected by a mutex;
- per-task or pooled read-only connections opened with read-only flags;
- WAL mode retained;
- avoid holding the writer mutex for long scans.

The current `query_stream` already opens a read-only connection for one path.
Generalize that pattern to normal query/count/find reads.

Acceptance:

- concurrent read queries do not block each other on the writer mutex;
- a slow query cannot stall unrelated collection reads through the global
  connection lock.

### 1.4 Bulk Writes And Chunk Operations

Finish batching paths that still do one transaction per item:

- write desktop file chunks through `bulk_write`/`bulk_upsert`;
- prune desktop file chunks by primary-key prefix/range or explicit expected
  chunk ids, not by generic `file_id` query;
- keep generation verification by metadata where possible before querying
  chunk rows.

Acceptance:

- a K-chunk file causes one chunk write transaction, not K transactions;
- cleanup never scans the whole `desktop_file_chunks` table.

## Phase 2 - Daemon Idle And Status Hotpath

This phase directly targets the observed "idle burns a core" failure mode.

Tasks:

1. Move expensive lifecycle checks out of normal status snapshots.
2. Add a short TTL cache for duplicate process detection, or expose it only via
   an explicit lifecycle/doctor probe.
3. Remove `recover_business_os_app_queue_tasks_for_idle_status_snapshot()` from
   every status request; schedule it from a bounded maintenance loop with a
   minimum lease age and DB-change gate.
4. Cache or precompute queue counts/previews for status, instead of opening and
   scanning durable stores on every UI poll.
5. Keep Business OS health enrichment outside daemon IPC, as already started.

Acceptance:

- `ctox status --json` does not fork `ps` on normal cached polls.
- status IPC p95 below 100 ms while idle.
- status polling at 2 Hz does not measurably increase daemon CPU.
- `sample` no longer shows `runtime_lifecycle_alerts`,
  `matching_service_processes`, or backend residue checks as idle hotspots.

## Phase 3 - Projection Loops And DB Growth Control

Tasks:

1. Replace broad projection reconciliation loops with high-water marks:
   - `ctox_queue_tasks`;
   - `business_chats`;
   - knowledge table projections;
   - command consumer pending status.
2. Introduce collection-specific indexed reconciliation tables or direct SQL
   queries for hot projection checks instead of generic RxDB Mango queries until
   Phase 1 is complete.
3. Add retention policies:
   - command/event history retention;
   - completed queue-task projection retention;
   - stale desktop file chunk generations;
   - tombstones older than a safe replication horizon.
4. Add DB-size reports to `ctox doctor` or a dedicated performance command.

Acceptance:

- projection loops do O(changed rows) work per pass;
- core DB and Business OS RxDB size can be explained by top collections;
- stale chunks/tombstones do not grow without bound.

## Phase 4 - Browser CTOX DB And WebRTC Diagnostics

Tasks:

1. Throttle `transport-status` snapshot emission:
   - counters update synchronously;
   - heavy `getTransportStatus()` rebuild at most once per animation frame or
     250 ms;
   - emit only if there are observers or diagnostics UI is open.
2. Avoid `encodedSize()` allocating a new `TextEncoder` and full encoded buffer
   per frame.
3. Replace sidecar query-window invalidation full scans with a reverse
   `docId -> windowKeys` index.
4. Implement IndexedDB query planning for schema indexes, not only the
   `collectionLwtId` fast path.
5. Implement browser `count()` as a cursor count for indexable queries.
6. Make collection subscriptions apply changed-id deltas or targeted re-query
   windows instead of `find().exec()` on every change.

Acceptance:

- a chunk transfer no longer produces one heavy diagnostic snapshot per frame;
- browser main-thread allocation from sync diagnostics drops sharply in a
  frame-transfer smoke test;
- common selector queries do not call `allDocuments()`;
- browser `count()` does not materialize documents.

## Phase 5 - Business OS UI Hot Paths

Tasks:

1. Batch `business-chat` tracked message lookups by ids and debounce command /
   queue subscriptions.
2. Replace full chat `innerHTML` diffing with content signatures and append /
   reconcile behavior.
3. Arm chat scheduler intervals only while scheduled messages/countdowns exist.
4. Add Map indexes and debounced searches for Matching, Outbound, Buchhaltung,
   Customers, CV Print Builder, Conversations, and Spreadsheets.
5. Avoid full module reloads on unrelated collection changes; route change
   events by collection and visible view.

Acceptance:

- typing in module search fields does not trigger O(all records) recompute per
  keypress;
- idle browser shell has no permanent 1 second scheduler loop unless needed;
- module render smoke tests cover data sets large enough to catch O(N^2)
  regressions.

## Phase 6 - Business OS Store And Core Persistence SQLite

Tasks:

1. Add `PRAGMA synchronous=NORMAL` to:
   - `src/core/business_os/store.rs::open_store_connection`;
   - `src/core/persistence.rs::open_sqlite`.
2. Cache RxDB table column metadata for projection writes.
3. Reuse long-lived RxDB connections for projection writes.
4. Batch `push_collection_records` and other per-record write loops in one
   connection and transaction.
5. Add the missing `business_commands(module, command_type, status,
   observed_at_ms)` index if that command-completion scan remains active.

Acceptance:

- projection writes do not reopen DBs and re-run `PRAGMA table_info` per row;
- batch imports and browser push batches commit in bounded transactions;
- fsync count during projection bursts is reduced.

## Phase 7 - Communication, Execution, Inference, Mission/Report

These are not the first idle-CPU blockers, but they are confirmed review areas.

Communication:

- add `idx_stalwart_messages_mailbox_received`;
- split IMAP message listing into headers/flags projection and body-on-demand;
- avoid fresh SQLite connections per mailserver hot call;
- use UID watermarks and targeted header search instead of full mailbox UID/body
  polling.

Execution gateway:

- inspect event method before cloning/deserializing streamed event payloads;
- accumulate API cost usage in memory and write once per turn;
- reuse tokenization preflight results.

Inference:

- keep ggml descriptor arenas or graph contexts alive across decode steps;
- investigate graph reuse/reserve-once for fixed decode shapes;
- move argmax and token embedding work off the host hot path where possible.

Mission/report:

- batch ticket assignment hydration;
- thread one DB connection through Business OS ticket projection helpers;
- remove redundant sorts and per-row DB opens in report/queue helpers.

Acceptance:

- IMAP FETCH FLAGS and STORE avoid loading full bodies;
- event delta streams avoid clone/deserialize work for ignored event types;
- per-token local inference CPU overhead is measurably lower;
- ticket projections do not reopen the same DB repeatedly per pass.

## Release Discipline

Do not call the work complete after code changes alone. For each phase:

1. land focused tests with the change;
2. run targeted Rust/JS suites required by `AGENTS.md`;
3. push `main`;
4. build through `ctox upgrade --dev`;
5. verify the installed release symlink points at the new release;
6. sample the actual `ctox-real` process after startup work has finished;
7. record idle CPU, status latency, and DB-size evidence in the phase notes.

The currently attempted release after `ce5bb23d` did not complete locally: the
installer process was terminated by signal 15 during Cargo build, so that
release must not be treated as deployed until a clean `ctox upgrade --dev`
finishes and `current` points at the new release.

## Proposed Execution Order

1. Phase 0 measurement and guards.
2. Phase 2 status/idle hotpath cache, because it directly affects current
   operator pain and avoids measurement pollution.
3. Phase 1 native RxDB SQLite query compiler, count, and read-connection split.
4. Phase 3 projection high-water marks and retention.
5. Phase 4 WebRTC diagnostics throttling and browser query planning.
6. Phase 6 Business OS store/persistence SQLite batching and pragmas.
7. Phase 5 UI keystroke/render work.
8. Phase 7 remaining subsystem optimizations.

## Open Risks

- Generic Mango-to-SQL pushdown must preserve RxDB semantics. Start with a
  conservative expressible subset and keep Rust matcher validation for safety.
- JSON expression indexes can become fragile if schema paths are not canonical.
  Generate indexes from registered schema indexes and test drift.
- Status caching must not hide real duplicate daemon or stale PID problems.
  Use TTL plus explicit `doctor`/lifecycle probes.
- Retention must respect replication horizons. Do not delete tombstones or chunk
  generations until checkpoint/epoch safety is proven.
- Browser diagnostics throttling must not break existing advanced-status fields.
  Preserve the field surface and change only emission cadence.
