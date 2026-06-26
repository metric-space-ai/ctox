# CTOX Performance Subagent Review And Optimization Plan - 2026-06-26

Source review:
`/Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md`

Workspace reviewed:
`/Users/michaelwelsch/Documents/ctox.nosync`

Repo-local copy:
`docs/ctox-performance-review-2026-06-24.md`

The external source review and the repo-local copy were checked with `cmp` and
are byte-identical.

## Verdict

No. The 2026-06-24 performance review is not fully handled.

The current worktree has real fixes for several original hot paths, especially
native RxDB query-fetch fallbacks, read-only SQLite reads, bounded RxSubject
fanout, WebRTC diagnostics, browser IndexedDB upsert/bulkUpsert, Chat tracking,
mail body over-fetch, and mailserver SQLite connection reuse. It is still not
valid to claim that CTOX is idle-clean or performance-complete.

The remaining structural blockers are:

- installed `ctox-real` idle evidence after `ctox upgrade --dev` is still
  missing;
- native SQLite still has unsupported normal `query()` and `count()` fallback
  scans outside the strict WebRTC `rxdb.query.fetch` path;
- Business OS projection and repair loops are reduced, but still rely on
  aggregate stamps, periodic wakeups, cursor-less repair windows, and some
  direct projection writer opens;
- browser-runtime ephemeral collections (`browser_frames`,
  `browser_input_events`) lack the retention, indexing, and payload-redaction
  policy needed for long-running idle safety;
- chunk and file paths are demand-oriented now, but still need stronger burst
  batching, physical DB-growth control, and scoped bridge release;
- Browser demand-stream collectors can outlive peer loss;
- several Business OS module UI paths still do O(all records) work on
  keystrokes or broad change events;
- local inference and some mission/service tail work remain open.

## Review Method

This plan is based on:

- root guidance: `AGENTS.md`, `README.md`, `HARNESS.md`,
  `docs/architecture.md`;
- data-plane guidance: `docs/ctox-rxdb.md`,
  `src/core/rxdb/AGENTS.md`, and `src/apps/business-os/rxdb/AGENTS.md`;
- the external performance review dated 2026-06-24;
- existing plans in `docs/ctox-performance-optimization-plan-2026-06-25.md`,
  `docs/ctox-performance-optimization-plan-2026-06-26.md`, and
  `docs/ctox-comprehensive-performance-optimization-plan-2026-06-26.md`;
- four read-only subagent reviews:
  - native RxDB/SQLite adapter and Business OS DB growth;
  - daemon/service idle loops and file-access wake paths;
  - browser RxDB/WebRTC/shell demand-stream lifecycle;
  - gap matrix against the 2026-06-24 review;
- direct spot checks of the current worktree.

Subagents did not edit files. The main review additionally made small
measurement-surface corrections:

- native RxDB SQLite statement counters now include checkpoint and
  changed-since reads;
- the perf probe default idle assertions now include write transaction and
  writer-lock activity counters.

## Current Coverage Summary

Status terms:

- `fixed`: the exact reviewed path is addressed in inspected code and has
  targeted test or guard evidence.
- `fixed for exact path`: the cited hot path is fixed, but adjacent work remains.
- `partial`: reduced, but still architecturally incomplete or missing release
  evidence.
- `open`: reviewed behavior is still present.

| Finding | Status | Current assessment |
| --- | --- | --- |
| H1 native RxDB non-PK scans | partial | Common selectors/counts compile to SQL and query-fetch rejects unsupported stream fallback, but normal unsupported `query()`/`count()` can still scan. |
| H2 WebRTC status per frame | fixed for exact path | Status emissions are coalesced and heavy diagnostics are opt-in. Keep observer/fanout counters in installed evidence. |
| H3 IMAP FETCH/STORE body over-fetch | fixed for exact body path | Summary/body split and mailbox index exist. Large-folder pagination, first import, UIDVALIDITY, and IDLE/delta work remain. |
| H4 Business Chat tracked-message N+1 | fixed for exact path | Chat tracking batches command/task reads, coalesces sync, and only watches command/queue while active tracked messages exist. DOM/layout work remains. |
| H5 Matching keystroke recompute | open | Matching still needs lookup maps, cached search haystacks, debounce, and DOM reconciliation. |
| H6 Outbound pipeline recompute | open | Outbound still needs memoized pipeline/company views and `pipelineByCompanyId`. |
| M1 native count materializes docs | partial | SQL count exists for compilable selectors; unsupported fallback counts are slow and observable but still scan. |
| M2 single SQLite connection mutex | partial | Key read paths use read-only WAL connections; writer serialization, checkpoint-status writer lock, and read-connection pooling remain. |
| M3 query-fetch full scan | fixed | WebRTC query-fetch refuses unsupported Rust matcher fallback before data chunks. |
| M4 queue/chat projection reconcilers | partial | Active-status and tracking filters plus batching exist; persisted changed-id/high-water repair windows remain. |
| M5 desktop chunk prune scan | fixed for exact native prune | Native stale generation prune uses key/range paths. Broader chunk retention and bloat control remain. |
| M6 per-chunk write transactions | partial | Native eager chunk writes use bulk upsert; browser upload/import chunk batching and bridge lifecycle remain. |
| M7 demand-cache invalidation scan | fixed for exact path | Reverse document-to-window refs and batch invalidation exist. Keep behavioral large-batch guards. |
| M8 browser upsert overhead | fixed | IndexedDB upsert and bulkUpsert use batched readwrite paths with dist-level smoke coverage. |
| M9 subscription full re-query | partial | `collection.$` and primary-key `findOne().$` apply deltas; complex query subscriptions still re-exec. |
| M10 browser `allDocuments()` fallback | partial | Primary-key/schema-index paths exist; non-indexed selectors still fall back. |
| M11 inference arena allocation | open | Qwen decode still needs persistent descriptor arenas or reusable contexts. |
| M12 inference graph rebuild | open | Graph/context reuse remains. |
| M13 stream delta clone/deserialize | fixed for delta/no-op path | Direct session filters high-frequency ignored events before payload clone/deserialization. Cost telemetry batching remains. |
| M14 blocking file fetch | fixed | Production file fetch uses bounded worker/channel/backpressure instead of parking tokio workers. |
| M15 unbounded RxSubject fanout | fixed for native fanout | Bounded broadcast, lag markers, query-buffer invalidation, replication `RESYNC`, and lag counters exist. Slow-peer integration soak remains. |
| M16 mailbox index | fixed | `stalwart_messages` has mailbox/received index. |
| M17 mailserver connection churn | fixed for broad hot path | Public store hot paths now use thread-local cached connections; tests cover IMAP SELECT/FETCH/STORE plus SMTP/CalDAV/CardDAV/greylist sequences. |
| M18 send verification full RFC822 | fixed | Header search/header fetch replaces repeated full-body polling. |
| M19 email full UID scans | partial | UID watermarks reduce steady state; first import, UIDVALIDITY, IDLE, and provider delta tokens remain. |
| M20 ticket assignment N+1 | partial | Self-work list/projection hydration is set-based; single-load and queue bridge paths remain. |
| M21 ticket projection DB reopens | fixed for direct projection | Direct projection buckets reuse one ticket DB connection. Non-projection helpers still need audit. |
| M22 chat full DOM rebuild | partial | Some in-place paths exist, but no-op sync can still build/compare HTML. |
| M23 forced reflow in drag | open | Layout read/write batching remains. |
| M24 sync diagnostics fanout | partial | Diagnostics are coalesced and skinny by default, but sanitize/record/fanout still need counters. |
| M25 spreadsheet HyperFormula rebuild | open | Persistent engine and changed-cell updates remain. |
| M26 matching requirements scans | open | Maps/debounce/DOM reconcile remain. |
| M27 Buchhaltung joins per render | open | Pre-aggregated maps and targeted reloads remain. |
| M28 customers search full render | open | Debounced center-only rendering remains. |
| M29 projection writer reopen/table_info | partial | Writer cache covers several batch paths; command/file/release/direct status paths and production counters remain. |
| M30 SQLite synchronous NORMAL | fixed for checked central stores | Core, Business OS store, native RxDB, and mailserver checked paths set NORMAL/WAL. Add guards for future direct helpers. |
| M31 runtime status polling | fixed for normal status path | Process scans and app recovery are cached/gated. Installed daemon idle evidence remains. |

Important low findings now fixed or strongly reduced:

- WebRTC `encodedSize()` no longer allocates `TextEncoder` buffers on the hot
  path and is guarded by `frame-chunking-smoke`.
- Incoming WebRTC frame ACK bookkeeping now tracks contiguous sequence
  incrementally instead of rescanning from zero per chunk.
- Browser shared-room protocol collection schema/checkpoint maps use bounded
  parallel collection fanout.
- Native `bulk_write` current-state reads use batched ID lookup.
- File-backed per-collection RxDB safety polls are removed from production
  idle paths.

Important low findings still open or partial:

- browser chunk upload batching and chunk bridge ownership;
- service-loop working-hours canonicalization and process-mining authorizer
  env lookup;
- mission queue/report DB-open and hydration tails;
- UI shell layout/listener/progress work;
- local inference Metal/host-side lows.

## Reconfirmed Subagent Findings

### Native RxDB, SQLite, And DB Growth

- `browser_frames` can explain idle CPU and DB growth when it grows large:
  expired-frame GC filters by `expires_at_ms`, but the schema is not indexed for
  that standalone predicate, and removal uses tombstones that can retain large
  JSON/base64 payloads until cleanup.
- `browser_input_events` appears unbounded and is missing a composite
  `(session_id, status, seq)` style index for the hot drain/count path. Consumed
  and failed rows need a retention policy.
- Desktop chunk external drains are no longer idle polls, but the batch size of
  2 rows for `desktop_file_chunks` creates many small wakeups during large chunk
  bursts or tombstone bursts.
- Desktop chunk maintenance still has a potentially broad stale-live-chunk
  `NOT EXISTS`/`json_extract` path that needs an `EXPLAIN QUERY PLAN` guard and
  probably a more direct candidate index.
- `replication_checkpoint_status` still uses the shared writer mutex. That can
  serialize handshakes/status over many collections and should move to a
  read-only or cached checkpoint path.
- Bulk write SQL still prepares/formats insert/update statements per document;
  prepared statement reuse inside one transaction remains a tail optimization.

### Daemon Idle

- The Business OS native peer is a projection and replication service, not a
  passive idle process. Idle safety depends on source stamps and loop counters,
  not just absence of queue work.
- Browser-runtime maintenance backs off after empty work, but it needs explicit
  heartbeat/perf counters for active ticks, pending-event queries, frame-GC
  rows, and timeout wakes.
- Core router gates are vulnerable to irrelevant Core-DB/WAL stamp churn. If
  any idle path keeps touching core DB, it can reopen route preflights and
  durable queue probes.
- Desktop file indexing still wakes periodically and can full-scan after the
  fallback interval or repeated root stamp churn.
- Channel sync treats non-null pairing payloads as activity; adapters that
  report unchanged pairing can suppress backoff.

### Browser RxDB And WebRTC

- Query/File demand collectors can hang across peer loss. `removePeer()` drops
  peer state but does not reject active query/file collectors and file demand
  loading lacks a complete abort path.
- Chat attachment staging releases `desktop_file_chunks`, but command-bus
  attachment dependencies can start that collection again and do not
  symmetrically stop it after dispatch.
- First IndexedDB schema-index backfill can scan a large collection after
  upgrade. It is not an idle loop, but it explains post-upgrade CPU spikes.
- Complex query subscriptions still re-exec after change bursts. Primary-key
  and collection-level delta paths help, but broad selectors remain a multiplier.

## Optimization Plan

### P0 - Installed Idle Evidence Gate

Do not claim an idle fix until this is green through the real installed dev
path.

Tasks:

1. Push main-ready fixes.
2. Run `ctox upgrade --dev`.
3. Record git commit, installed binary path, build/release id, `ctox-real` PID,
   and DB file sizes.
4. Run `src/tools/perf/ctox_perf_probe.py --assert-idle` for:
   - fresh daemon, no browser;
   - Business OS open and synced;
   - after file access grant;
   - File Viewer materialize/read;
   - CV Print Builder open and idle;
   - 10 minutes no user input after warmup.
5. Capture `sample` or `spindump` for any scenario over budget.

Acceptance:

- `ctox-real` averages below 2 percent CPU over 5 minutes.
- 10-minute no-input p95 stays below 5 percent CPU.
- `ctox status --json` p95 stays below 100 ms.
- No DB/WAL file grows monotonically during idle.
- No SQLite fallback, write transaction, writer-lock, or loop row counter grows
  continuously while sources are unchanged.

### P0 - Ephemeral Browser Runtime Retention And Indexes

This is a direct DB-growth and post-browser-idle risk.

Tasks:

1. Add schema indexes for:
   - `browser_frames.expires_at_ms`;
   - `browser_input_events` hot drain/count shape, at minimum
     `(session_id, status, seq)` or equivalent schema-index order.
2. Redact or physically remove expired `browser_frames` payloads instead of
   retaining large tombstoned JSON/base64 payloads.
3. Add retention for consumed/failed `browser_input_events`.
4. Add loop counters for browser-runtime maintenance:
   - active ticks;
   - timeout wakes;
   - input rows drained;
   - frame rows expired;
   - GC rows physically removed/redacted.
5. Add DB diagnostics for row counts, tombstones, JSON payload bytes, WAL size,
   and freelist pages for both collections.

Acceptance:

- Expired-frame GC uses an indexed plan.
- A 10-minute idle browser session does not grow `browser_frames` or
  `browser_input_events`.
- Browser-runtime heartbeat deltas stay at zero when no input/frame work exists.

### P1 - Native RxDB SQLite Closure

Tasks:

1. Expand `compile_query_sql` and `compile_count_sql` for real Business OS
   selectors still falling back.
2. Integrate existing query-planner concepts instead of growing ad hoc selector
   handling indefinitely.
3. Keep `rxdb.query.fetch` strict: unsupported stream queries must return
   `QUERY_NOT_SUPPORTED`, not scan.
4. Move `replication_checkpoint_status` to a read-only connection or cached
   checkpoint snapshot.
5. Add prepared statement reuse inside `bulk_write` transactions for insert and
   update rows.
6. Keep runtime counters for:
   - fallback calls and rows visited;
   - statements executed;
   - write transactions;
   - writer lock wait/held time;
   - read-only open failures;
   - external poll reads.
7. Evaluate pooled/thread-local read-only connections to avoid per-read open
   overhead without reintroducing writer-lock contention.

Acceptance:

- Hot query tests assert intended indexes with `EXPLAIN QUERY PLAN`.
- Installed idle probe shows no growing `query_fallback_rows_visited`.
- Checkpoint status no longer waits for the writer mutex in file-backed storage.

### P1 - Chunk, File, And Physical DB Growth

Tasks:

1. Increase/shape `desktop_file_chunks` external drain batches by rows and bytes
   so large bursts do not self-signal in tiny two-row batches.
2. Keep normal browser file viewing on `rxdb.file.fetch`, not background chunk
   pull or query demand.
3. Add `EXPLAIN` guards for stale live chunk cleanup paths that use
   `NOT EXISTS` or JSON expressions.
4. Add physical retention metrics:
   - live/deleted/blanked chunk rows;
   - JSON payload bytes;
   - stale generation rows;
   - WAL and freelist pages.
5. Define safe checkpoint/vacuum policy as explicit maintenance, not hidden idle
   work.
6. Make browser upload/import chunk writers batch by collection and release
   transient chunk bridges after flush.

Acceptance:

- File access/materialize/import scenarios show bounded rows touched and no
  idle DB growth after the burst completes.
- `desktop_file_chunks` is inactive unless a browser upload/import surface owns
  an explicit temporary lease.

### P1 - Browser Demand Stream And Collection Lifecycle

Tasks:

1. Add abort/reject handling for query and file demand collectors when a peer is
   removed or the DataChannel closes.
2. Extend `FileDemandLoader` with explicit abort/cancel semantics matching
   query demand loading.
3. Make `removePeer()` reject all in-flight query/file demand streams owned by
   that peer and free slots/counters.
4. Done on 2026-06-27 for central chunk bridge ownership: direct
   `startCollection()` calls for `desktop_file_chunks`, `document_blob_chunks`,
   and `spreadsheet_blob_chunks` now fail with
   `DEMAND_ONLY_COLLECTION_REQUIRES_LEASE` unless a scoped lease already owns
   the collection start. `startModule()` skips those collections, and
   `leaseCollection()` remains the activating/releasing path.
5. Ensure command bus, CV Builder, imports, and future upload surfaces release
   chunk bridges after `awaitInSync`/dispatch.
6. Move large schema-index backfills into idle slices or an explicit upgrade
   task with progress/cancellation.
7. Add complex query subscription counters and dev diagnostics for full
   re-exec paths.

Acceptance:

- Peer-close during file/query demand fails fast and leaves zero in-flight
  collectors.
- Attachment dispatch does not leave `desktop_file_chunks` active after flush.
- Direct module/runtime `startCollection()` cannot activate large chunk
  collections without an explicit scoped lease.
- Backfill CPU is bounded and observable after upgrade.

### P1 - Business OS Projection And Repair Delta Windows

Tasks:

1. Replace aggregate summary stamps with maintained change generations or
   persisted changed-id cursors.
2. Repair queue/chat projections from changed task/chat/command ids, not broad
   aggregate pages.
3. Move remaining command acceptance/completion/failure, file/share/release, and
   document/version projection call sites onto `RxdbProjectionWriterCache`.
4. Cache per-table metadata and statement shapes so `PRAGMA table_info` is not
   paid per record.
5. Batch `push_collection_records` command paths where semantics allow.
6. Add projection burst counters for SQLite opens, PRAGMA loads, statements,
   rows touched, and writer-lock held time.

Acceptance:

- Unchanged idle projection rounds do zero table-sized work.
- Projection writer bursts open SQLite O(collections), not O(records).
- Queue/chat repair row counters remain zero in unchanged idle.

### P2 - Daemon Idle Gates And Status Tail

Tasks:

1. Harden core router gates against unrelated Core-DB stamp churn.
2. Treat unchanged channel `pairing` payloads as no activity, so sync backoff can
   grow.
3. Convert desktop-file fallback scans into a dirty-root watcher/safety path:
   short interval only for known dirty roots, long fallback otherwise.
4. Add source/table-change gates for progressive mission approval/self-work
   maintenance.
5. Cache canonical working-hours root.
6. Capture process-mining authorizer config once instead of reading env on each
   SQLite authorizer callback.
7. Keep heartbeat/status baseline cheap and explicitly measured.

Acceptance:

- No queue/router/app-recovery scanner wakes repeatedly when only irrelevant
  DB stamps changed.
- Desktop-file indexing does not recurse through shared roots on every short
  idle cadence.
- Channel sync backs off when adapters return unchanged data.

### P2 - Browser Shell And Module UI Hot Paths

Tasks:

1. Chat:
   - content signatures per open chat window;
   - append/reconcile changed nodes;
   - avoid `innerHTML` serialization on no-op sync;
   - coalesce alignment behind one `requestAnimationFrame`.
2. Window manager:
   - read geometry once per frame;
   - batch style writes after reads;
   - avoid snap/shadow layout reads after writes.
3. Reporter/startup:
   - replace duplicate high-frequency pointer listeners with one throttled path;
   - stop progress creep near cap or use CSS transition.
4. Matching:
   - add match/object maps;
   - cache normalized haystacks;
   - debounce search;
   - reconcile DOM.
5. Outbound:
   - compute current pipeline/companies once per render;
   - build `pipelineByCompanyId`;
   - prevent broad `loadAll()+render()` for unrelated events.
6. Spreadsheet:
   - keep one HyperFormula engine;
   - update changed cells with `setCellContents`;
   - update only cells reported changed.
7. Buchhaltung/customers/CV/conversations:
   - pre-aggregate maps;
   - render only affected panes/rows;
   - use delegated listeners and data signatures.

Acceptance:

- Fixture smokes seed at least 1,000 records for Matching, Outbound,
  Buchhaltung, Customers, and representative Chat tracking.
- Search keystrokes are debounced and stay inside the chosen UI budget.
- No module search box performs synchronous O(all records) work on every
  keydown.

### P3 - Mail, Mission, Execution, And Inference Tail

Tasks:

1. Mail:
   - add SQL-paginated summary/sequence resolution for large folders;
   - persist UIDVALIDITY-aware account/folder watermarks;
   - add IMAP IDLE or provider delta-token paths where available.
2. Mission/tickets/queue:
   - batch queue bridge task/ticket hydration;
   - preload spill-candidate signatures with set-based queries;
   - pass open plan DB connections through due-step batches;
   - stop re-querying metadata already selected by cleanup lists.
3. Execution:
   - pass exact prompt token preflight results into direct session;
   - buffer API cost telemetry to one write per turn where possible;
   - reduce provider adapter transcript clone/parse/serialize churn.
4. Inference:
   - persistent descriptor arenas or reusable ggml contexts;
   - graph/context reuse keyed by legal decode shapes;
   - benchmark host time, allocation count, graph builds, and malloc/free bytes.

Acceptance:

- Large mailbox first import is bounded and does not repeat full UID scans in
  steady state.
- Mission/ticket/report helpers avoid per-row DB opens in projection paths.
- Per-token inference host overhead is measured before and after any change.

## Suggested Verification Commands

Narrow checks for the current plan and recent measurement changes:

```sh
python3 -m py_compile src/tools/perf/ctox_perf_probe.py
cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml
CARGO_TARGET_DIR=/tmp/ctox-rxdb-counters-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance -- --nocapture
node src/apps/business-os/rxdb/tests/frame-chunking-smoke.mjs
node src/apps/business-os/rxdb/tests/handshake-checkpoints-parallel-smoke.mjs
node src/apps/business-os/rxdb/tests/run-all.mjs
```

Release evidence gate:

```sh
ctox upgrade --dev
python3 src/tools/perf/ctox_perf_probe.py --assert-idle --cpu-samples 300 --cpu-interval 1 --pretty
```

Run that gate for the P0 scenarios listed above, not only a fresh daemon.

## Immediate Work Order

1. Finish and verify native SQLite runtime counters and installed perf-probe
   thresholds.
2. Add browser-runtime retention/indexes for `browser_frames` and
   `browser_input_events`.
3. Done for Browser demand-stream abort on peer loss and central demand-only
   chunk-bridge scoped ownership; remaining browser work is complex query
   re-exec/fallback counters and future upload/import guard coverage.
4. Move checkpoint status off the shared writer mutex.
5. Replace projection aggregate stamps/repair sweeps with changed-id cursors.
6. Add chunk physical DB-growth reporting and safe maintenance policy.
7. Finish H5/H6 module hot paths with 1k-record perf smokes.
8. Run `ctox upgrade --dev` and the installed idle evidence gate before any
   release or idle-clean claim.
