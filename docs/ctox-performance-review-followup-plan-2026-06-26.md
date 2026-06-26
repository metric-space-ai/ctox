# CTOX Performance Review Follow-up Plan - 2026-06-26

Source review:
`/Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md`

Workspace:
`/Users/michaelwelsch/Documents/ctox.nosync`

## Verdict

No. The 2026-06-24 performance review is not fully handled.

The current codebase contains real fixes and strong reductions for several of
the original high-impact paths, but it is not release-clean and it is not
valid to claim that the installed `ctox-real` daemon is idle-clean yet.

The remaining release blockers are structural:

- native SQLite still has unsupported Mango fallback scans outside strict
  `rxdb.query.fetch`;
- queue/chat repair no longer turns a small local queue reconcile into global
  repair fanout, but cursor-based repair windows are still missing;
- repair source stamps still use table-sized summaries instead of true delta
  cursors;
- browser RxDB subscriptions still re-run full queries in important paths;
- browser chunk writes remain sequential in several upload/import paths;
- file demand fetch is demand-based but not range-efficient end to end;
- there is no installed `ctox upgrade --dev` idle proof after file access,
  File Viewer materialization, CV Builder, and 10 minutes of no user input;
- there is no release gate that fails when `ctox-real` consumes a sustained
  core while idle.

## Review Method

This follow-up was based on:

- root repository guidance in `AGENTS.md`;
- the external review dated 2026-06-24;
- existing plans in `docs/ctox-performance-optimization-plan-2026-06-25.md`
  and `docs/ctox-performance-optimization-plan-2026-06-26.md`;
- direct inspection of the current source;
- four read-only subagent reviews:
  - native RxDB SQLite adapter;
  - Business OS native peer, store, queue/chat repair, files;
  - browser Business OS RxDB/WebRTC/modules;
  - release and verification gates.

No subagent edited files. Their findings agree on the main conclusion: the
old review is represented in the plans, but a significant part remains open
or only partially fixed.

## Current Coverage Summary

Legend: `fixed`, `partial`, `open`.

| Finding | Status | Current assessment |
| --- | --- | --- |
| H1 native RxDB non-PK scans | partial | Common selectors compile to SQL, but SQLite does not fully use `query_planner.rs`; unsupported normal queries/counts can still scan. |
| H2 WebRTC status per frame | fixed for exact path | Status is throttled/coalesced and heavy diagnostics are opt-in; release probes still need emit/fanout counters. |
| H3 IMAP FETCH/STORE body overfetch | fixed for exact body path | Exact mailbox summary/body split is implemented; large-mailbox sequence pagination remains under mail follow-up work. |
| H4 business chat tracked-message N+1 | fixed for exact path | Browser `syncTrackedMessages` batches command/task reads into one query per collection, coalesces subscription-triggered sync, and command/queue tracking watchers only run while active tracking exists; broader Chat DOM/layout/listener cleanup remains. |
| H5 Matching per-keystroke recompute | open | Maps, cached haystacks, debounce, and DOM reconciliation remain. |
| H6 Outbound per-row pipeline recompute | open | Memoized pipeline and `pipelineByCompanyId` remain. |
| M1 RxDB count materializes docs | partial | SQL counts exist for expressible paths; fallback counts can still scan/materialize. |
| M2 single SQLite connection mutex | partial | Read-only WAL paths exist for key reads; writes, some fallbacks, and backpressure remain. |
| M3 query-fetch full scan | fixed for native query-fetch | Unsupported SQLite stream fallbacks are rejected before data chunks. |
| M4 projection reconcilers | partial | Idle gates exist, but repair windows and source stamps are still not true delta cursors. |
| M5 desktop chunk prune full scan | fixed for exact native prune | PK/range prune is guarded; broader retention remains. |
| M6 chunk writes one transaction per chunk | partial | Native eager writes use bulk upsert; browser chunk uploads remain sequential. |
| M7 demand-cache window scans | mostly fixed | Reverse refs and batch invalidation exist; remaining eviction/local-write work stays open. |
| M8 browser upsert transaction overhead | fixed | `storage-indexeddb.upsert()` now reads the existing row once inside one readwrite transaction, writes once, returns the written document, and is guarded by a dist-level storage smoke. |
| M9 subscriptions full re-query | partial | `collection.$` now applies change payloads to an in-memory snapshot and `findOne(primary).$` ignores unrelated changed IDs; complex query subscriptions still re-run full queries. |
| M10 browser `allDocuments()` fallback | partial | Primary-key/schema-index/bounded paths exist; non-indexed fallbacks remain. |
| M11 inference arena overhead | open | Persistent descriptor arenas remain. |
| M12 inference graph rebuild | open | Graph/context reuse remains. |
| M13 streamed event clone/deserialize | fixed for stream delta/no-op path | Direct session now inspects the method/payload event kind before cloning payloads, drops high-frequency delta/no-op events before deserialization, and keeps consumed agent-message parsing covered by tests. Cost telemetry batching and broader transcript-copy work remain separate execution-tail work. |
| M14 blocking file-fetch stream | fixed | File-fetch no longer parks a tokio worker with `block_on`/sleep. |
| M15 unbounded RxSubject fanout | fixed for native RxSubject fanout | Native `RxSubject` uses bounded broadcast fanout, lag counters, storage lag markers, query-buffer invalidation, replication `RESYNC` mapping, native peer/perf-probe surfacing for process-wide lag totals, and targeted slow-peer checkpoint-recovery coverage. Installed/integration slow-peer soak evidence remains release work. |
| M16 mailbox index | fixed | Mailbox/received index and query-plan guard exist. |
| M17 mailserver connection reuse | open | Broad hot-path `with_connection` reuse remains. |
| M18 send-verification body fetch | fixed | Header search/header fetch replaces full RFC822 polling. |
| M19 email full UID scans | partial | UID watermarks reduce steady state; first import, UIDVALIDITY, IDLE, provider delta tokens remain. |
| M20 ticket assignment N+1 | partial | Self-work list/projection hydration batches latest assignment lookup; single-load and broader ticket/queue bridge paths remain. |
| M21 ticket projection DB reopens | fixed for direct projection | Direct Business OS ticket projection buckets now reuse one ticket DB connection, including control bundles; non-projection ticket/queue helper audits remain separate. |
| M22 chat full DOM rebuild | partial | Some in-place paths exist, but no-op sync can still build message HTML and compare serialized `innerHTML`; signature/append-only reconcile remains. |
| M23 forced reflow in drag | open | Geometry read/write batching remains. |
| M24 sync diagnostics fanout | partial | Diagnostics are coalesced, but transport status still enters sanitize/record/fanout logic and needs observer/fanout counters. |
| M25 spreadsheet HyperFormula rebuild | open | Persistent engine and changed-cell updates remain. |
| M26 matching requirements scans | open | Maps/debounce/DOM reconcile remain. |
| M27 Buchhaltung joins per render | open | Pre-aggregated maps and targeted reloads remain. |
| M28 customers search full render | open | Debounced center-only render and shared summaries remain. |
| M29 projection writer reopen/table_info | partial | Cached writer covers repair/fanout paths; direct command/status paths remain. |
| M30 synchronous=NORMAL | fixed for checked central stores | `open_store_connection` and core persistence set `synchronous=NORMAL`; keep a guard for future direct SQLite helpers. |
| M31 status ps/proc scan | fixed for status idle path | Normal process scan is cached; explicit lifecycle/probe scans remain intentional. |

Low findings are only partially reduced. Remaining important low buckets:

- `push_collection_records` command-path batching and production counters;
- process-mining authorizer env lookup;
- working-hours canonicalization;
- browser `encodedSize`, browser chunk upload batching, chunk reassembly, and
  local push scan/fallback counters after the completed local-write trigger
  coalescing;
- mission/report N+1 and DB reopen cleanup;
- UI shell idle schedulers and module full reloads.

## Confirmed Improvements

These improvements are real and should be preserved with regression tests:

1. Native SQLite query/count/query-fetch paths compile common selectors into
   SQL with `WHERE`, sort, `LIMIT`, `OFFSET`, and `COUNT(*)`.
2. File-backed query, find-by-id, changed-since, and query-stream reads use
   read-only connections where possible.
3. `rxdb.query.fetch` rejects unsupported SQLite stream fallback queries
   instead of running Rust matcher scans on the WebRTC hot path.
4. Native file fetch uses bounded worker/channel backpressure instead of
   blocking runtime workers.
5. WebRTC transport status is throttled and heavy diagnostics are opt-in.
6. Demand-cache invalidation uses reverse document-to-window references.
7. Plain `ctox.file.materialize` with `payload.file_id` no longer wakes
   `desktop_file_chunks`.
8. CV Print Builder no longer treats `desktop_file_chunks` as a normal required
   or live collection.
9. Native materialized/eager file rescans check generation metadata before
   falling back to chunk completeness scans.
10. Native `bulk_write` current-state reads use a batched ID lookup instead of
    one `document_by_id` point query per written document.
11. The native peer heartbeat now exports loop counters and SQLite runtime
    counters for idle evidence collection.
12. Direct browser `startCollection()` now rejects large demand-only chunk
    collections unless a scoped `leaseCollection()` owns the bridge lifecycle.

## P0 - Evidence Gate Before Any Release Claim

The first blocking issue is not another code guess. It is missing proof from
the installed daemon.

Tasks:

1. Run `src/tools/perf/ctox_perf_probe.py --assert-idle` against the installed
   dev daemon after `ctox upgrade --dev`. The probe now enforces max average
   CPU, sustained CPU via p95, status p95 latency, native heartbeat loop/SQLite
   deltas, and DB file growth during the CPU sampling window.
2. Record separate scenario artifacts:
   - fresh daemon, no browser;
   - Business OS open and synced;
   - File Viewer materialize/read;
   - CV Print Builder open and idle;
   - after large file access grant;
   - 10 minutes of no input after warmup.
3. Make release evidence self-identifying:
   - git commit;
   - build/release id;
   - installed `current` symlink target;
   - `ctox-real` PID;
   - DB file sizes before/after;
   - native heartbeat before/after and deltas.
4. Add a release/manual CI workflow or release script step that runs the probe
   after `ctox upgrade --dev` and uploads JSON artifacts.

Acceptance:

- Installed `ctox-real` stays below 2 percent average CPU over 5 minutes and
  below 5 percent sustained CPU over the 10 minute no-input scenario.
- `ctox status --json` p95 stays below 100 ms.
- No loop counter shows continuous expensive work while sources are unchanged.
- No SQLite fallback/scan counter increases continuously while idle.
- The result is stored as an artifact, not just observed manually.

## P0 - Finish Queue/Chat Repair Delta Windows

Current problem:

- `reconcile_ctox_queue_task_projections` no longer runs an unfiltered first
  page; it now selects active queue statuses only. It still lacks a persisted
  changed-id/high-water cursor.
- Done on 2026-06-26: if local reconcile repairs local documents, it no longer
  calls `store::repair_queue_projections` afterward. A regression proves an
  unrelated old orphaned `ctox_queue_tasks` business record is not touched by
  local reconcile.
- Done on 2026-06-26: active queue rows are no longer hidden behind a first
  page of terminal rows; a regression seeds 600 terminal queue docs plus one
  active stale doc and proves the active doc is repaired.
- `store::repair_queue_projections` still exists as a global maintenance
  repair. It reads all non-deleted `business_records` where
  `collection = 'ctox_queue_tasks'`, then checks canonical queue/task/command
  state per row.
- Done on 2026-06-26: `business_chats` now carry indexed top-level tracking
  metadata, and `reconcile_business_chat_tracking_projections` selects only
  `tracking_active = true` documents instead of a broad first page. A
  regression seeds 600 inactive chat documents plus one active stale chat and
  proves the active chat is repaired.
- Legacy inline-artifact and fallback repair paths add more broad work.

Plan:

1. Replace queue/chat repair with persisted cursors:
   - last RxDB `ctox_queue_tasks` checkpoint;
   - last `business_commands` checkpoint;
   - last `business_chats` checkpoint;
   - canonical queue high-water or explicit dirty task ids.
2. Repair only changed task/chat/command ids.
3. Done on 2026-06-26: active Chat tracking command/task lookups are batched
   into one `business_commands` lookup and one `ctox_queue_tasks` lookup per
   repair pass, with a regression covering 40 active tracking messages.
4. Move legacy inline-artifact and fallback repairs to a bounded maintenance
   command with batch limits and its own telemetry.
5. Keep global audit repair as an explicit operator/maintenance action, never
   as an idle-loop side effect.

Acceptance:

- A single stale queue projection repairs only that projection and directly
  related command rows.
- Native heartbeat records zero queue/chat repair row work on unchanged idle.

## P0 - Finish Native SQLite Fallback Discipline

Current problem:

- SQLite still does not fully consume the existing `query_planner.rs` plan.
- Unsupported selectors such as `$regex`, some `$ne`, `$or`, and nested logical
  forms can fall back to read-only full-table Rust matcher scans in normal
  storage queries.
- Normal fallback `query()` scans are now visible through runtime counters for
  calls and rows visited.
- Fallback `count()` can still materialize results, but it is now marked
  `slow` instead of `fast` so `RxQuery` can reject it unless slow counts are
  explicitly allowed.

Plan:

1. Wire prepared `queryPlan` into the SQLite backend where it gives safe
   candidate narrowing.
2. Classify query shapes:
   - indexed SQL;
   - bounded SQL candidate plus Rust post-filter;
   - explicitly rejected on hot paths;
   - slow maintenance-only fallback.
3. Done on 2026-06-26 for the first runtime counters: normal SQLite fallback
   queries now expose fallback calls and visited rows. Remaining diagnostic
   counters should add row-decode and byte estimates.
4. Add hard guards for large Business OS collections:
   - `business_commands`;
   - `ctox_queue_tasks`;
   - `business_chats`;
   - `business_records`;
   - `desktop_files`;
   - `desktop_file_chunks`;
   - blob chunk collections.
5. Done on 2026-06-26: fallback `count()` reports slow mode. Remaining work is
   to remove or explicitly allow any idle/UI caller that still reaches this
   path.

Acceptance:

- Indexed hot selectors use guarded SQL plans.
- Unsupported interactivity/WebRTC queries fail clearly instead of scanning.
- Any remaining fallback scan is visible in the probe and bounded by policy.

## P1 - Native Peer Delta Scheduling

Current problem:

- Many loops are source-stamped, but some stamps still compute
  table-sized `COUNT/SUM/MAX` summaries or metadata hashes on a cadence.
- Desktop file indexing is better, but still periodic without watcher or
  durable dirty-root events.
- Desktop file maintenance after file-share bursts no longer runs under the
  native RxDB write lock or deserializes every live `desktop_files` row to find
  unsafe paths; the remaining gap is event-driven scan/maintenance scheduling
  and installed idle evidence.

Plan:

1. Replace aggregate repair stamps with explicit generations or high-water
   cursors.
2. Persist dirty queues for:
   - queue tasks;
   - business commands;
   - business chats;
   - file roots;
   - module lifecycle/release metadata.
3. Use filesystem watchers or root fingerprints for desktop/notes roots.
4. Keep slow fallback scans, but make them rare, budgeted, and measurable.
5. Add heartbeat deltas for rows visited, rows decoded, lock wait/hold time,
   and write batches per loop.

Acceptance:

- No unchanged idle loop reads table-sized metadata.
- File roots do not recurse or scan live/tombstone RxDB docs unless dirty or
  inside the documented slow fallback window.

## P1 - File Access, Range Fetch, And DB Growth

Current problem:

- Materialize/read paths are much better, but file demand fetch still gathers
  chunk metadata broadly for a file and applies ranges late.
- Chunk/blob payloads remain inline in SQLite JSON.
- Tombstones, freelist, and WAL growth lack a complete retention contract.

Plan:

1. Make `rxdb.file.fetch` range-aware:
   - map requested byte ranges to chunk indexes;
   - read only required chunk ids;
   - decode only required bytes;
   - stream without building full chunk vectors.
2. Add browser range/stream consumers for previews/imports where full blobs are
   not required.
3. Move large file/chunk payloads toward a content-addressed runtime blob store:
   - RxDB keeps manifests, hashes, refs, sizes, retention metadata;
   - only small payloads remain inline.
4. Define retention:
   - live generations retained by policy;
   - referenced attachments retained by linked collection/record;
   - stale chunks and tombstones physically deleted only after replication
     horizon;
   - WAL checkpoint/freelist shrink only in explicit idle maintenance.
5. Add DB diagnostics for:
   - live/deleted row counts;
   - tombstone reasons;
   - blanked payload bytes;
   - WAL/SHM/freelist;
   - top tables/collections by payload bytes.

Acceptance:

- Opening or previewing a small range of a large file does not read/decode all
  chunks.
- Repeated file share/update/delete cycles have bounded retained bytes after
  documented maintenance.
- Offline-browser reconnect tests prove retention does not delete required
  tombstones or referenced attachments.

## P1 - Projection Writer And Command Outcome Batching

Current problem:

- `RxdbProjectionWriterCache` covers several repair/fanout paths.
- Direct command/status/lifecycle/file/release paths still use single-record
  upsert helpers that can reopen SQLite and re-read table metadata.

Plan:

1. Thread `RxdbProjectionWriterCache` through:
   - command completion/failure;
   - control command acceptance;
   - file/share command fanout;
   - app/module release metadata;
   - lifecycle event projection.
2. Add statement/open counters around:
   - `upsert_rxdb_collection_record`;
   - `push_collection_records`;
   - command outcome projection paths.
3. Batch remaining `push_collection_records` command paths where semantics allow.
4. Add a 100-row regression proving O(tables) metadata loads and O(1) DB opens
   per projection pass.

Acceptance:

- A command burst does not create one DB open and one `PRAGMA table_info` round
  per projected row.

## P1 - RxSubject Backpressure

Current problem:

- The native unbounded per-subscriber channel implementation is fixed.
- Remaining release work is proving `rxdb_subjects.lagged_items_total` stays
  quiet in normal idle and running an installed/integration slow-peer soak over
  the checkpoint resync path.

Plan:

1. Done on 2026-06-26: replace native `RxSubject` fanout with bounded broadcast
   queues.
2. Done on 2026-06-26: storage overflow emits a lagged signal that forces query
   buffer invalidation and replication checkpoint resync.
3. Done on 2026-06-26: surface native lag counters in peer performance
   snapshots and `ctox_perf_probe.py --assert-idle`.
4. Done on 2026-06-26: add targeted slow-peer coverage proving a lagged
   master-change subscriber receives `RESYNC` and recovers all missed docs via
   `master_changes_since`. Installed/integration soak evidence remains.

Acceptance:

- Slow subscribers do not cause unbounded memory growth.
- Dropped event ranges are recovered through checkpoint resync.

## P2 - Browser RxDB And WebRTC Hot Paths

Current problem:

- `storage-indexeddb.upsert()` no longer performs read/write/read.
- `bulkUpsert()` no longer loops through per-document facade `upsert()` calls.
- Complex query subscriptions still re-run `find().exec()`. `collection.$` and
  primary-key `findOne().$` now apply changed-ID deltas.
- Several modules keep broad active collection sets.
- Local-write push trigger bursts are now debounced/coalesced before
  `pushToRemotePeers()`; broader browser chunk upload batching and explicit
  local push scan/fallback counters remain.
- `encodedSize()` and chunk reassembly still allocate or recompute.

Plan:

1. Done on 2026-06-26: collapse browser `upsert()` into one readwrite
   transaction and return the written doc.
2. Done on 2026-06-26: make `bulkUpsert()` a real batch path through storage
   `bulkUpsert()` with one readwrite transaction and one coalesced change
   event.
3. Partially done on 2026-06-26: `collection.$` and primary-key
   `findOne().$` apply changed-ID deltas. Add indexed-window/full-refresh
   semantics for complex query subscriptions and use them in shell/modules.
4. Add perf spies that fail on unexpected:
   - `allDocuments()`;
   - full live-query re-exec;
   - broad `desktop_file_chunks.find().exec()`;
   - local-write scan bursts and fallback floors beyond the debounced trigger
     path.
5. Reuse `TextEncoder` or avoid full encode just for byte counts.
6. Track chunk contiguous sequence incrementally.
7. Coalesce room transport status once per room instead of fanout-heavy
   per-collection snapshots.

Acceptance:

- A single changed document does not re-query or re-render a full collection.
- A file upload creates bounded browser write transactions and bounded push
  scans.

## P2 - Browser Modules And UI Work

Priority modules from the review and subagents:

1. `business-chat.js`
   - batch tracked command/task lookups with `$in`;
   - remove or disarm the 4 second interval when no active tracked messages
     exist;
   - use signatures/append-only DOM updates for message lists.
2. `cv-print-builder`
   - avoid full `business_commands`/`ctox_queue_tasks` refresh for status;
   - fetch only known command/task ids;
   - debounce search and use delegated row events.
3. Matching, Outbound, Buchhaltung, Customers, Conversations, Spreadsheets
   - build Maps once per data version;
   - cache search haystacks;
   - debounce search;
   - update affected DOM instead of full `innerHTML` rebuilds;
   - keep HyperFormula instances alive and update changed cells.
4. Support, Outbound, Shiftflow broad subscriptions
   - reduce live collections to visible/primary sources;
   - lazy-load detail data;
   - replace full `loadAll` with collection-specific partial refresh.
5. Shell idle schedulers
   - Done on 2026-06-26 for scheduled Chat messages: the scheduler now arms
     only while scheduled messages exist and clears the previous global
     1 second interval.
   - coalesce layout reads/writes behind one animation frame;
   - remove duplicate high-frequency activity listeners.

Acceptance:

- Representative large-fixture keystroke tests fail if a module recomputes or
  re-renders all records per keypress.
- Idle browser shell has no permanent high-frequency scheduler without visible
  work.

## P3 - Communication, Execution, Inference, Mission

These are not the most likely cause of the observed `ctox-real` file-share
idle CPU, but they remain open from the review and must not be lost.

Tasks:

1. Mailserver:
   - reuse hot-path SQLite connections broadly;
   - add UIDVALIDITY-aware watermarks;
   - add provider IDLE/delta-token paths where available.
2. Execution gateway:
   - Done on 2026-06-26: direct session inspects stream event kind before
     clone/deserialization and drops high-frequency delta/no-op events early;
   - accumulate API cost rows and write once per turn;
   - reuse tokenize preflight results;
   - avoid provider transcript clone/parse/re-serialize loops.
3. Inference:
   - persistent descriptor arenas;
   - graph/context reuse where shape permits;
   - host argmax/embedding hot path reduction;
   - keyed Metal PSO lookup.
4. Mission/report:
   - batch ticket assignment hydration;
   - thread one DB connection through projection helpers;
   - remove redundant sorts and per-row DB opens;
   - clean up orphaned report module islands.

## Required Verification Commands

Before any main/release claim:

```sh
cargo test --manifest-path src/core/rxdb/Cargo.toml
node src/apps/business-os/rxdb/tests/run-all.mjs
CARGO_TARGET_DIR=/tmp/ctox-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox idle_gate -- --nocapture
CARGO_TARGET_DIR=/tmp/ctox-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_ctox_queue_task_projections -- --nocapture
CARGO_TARGET_DIR=/tmp/ctox-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file -- --nocapture
CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox direct_session -- --nocapture
CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --test-threads=1 --nocapture
python3 -m py_compile src/tools/perf/ctox_perf_probe.py
```

For browser RxDB source changes:

```sh
npx -y esbuild@0.28.0 src/apps/business-os/rxdb/src/index.mjs --bundle --format=esm --outfile=src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs "--banner:js=// CTOX DB app-local bundle. Generated from src/apps/business-os/rxdb/src/index.mjs."
node src/apps/business-os/rxdb/tests/run-all.mjs
node src/apps/business-os/scripts/assert-rxdb-only.mjs
```

Installed release evidence:

```sh
git push origin main
ctox upgrade --dev
python3 src/tools/perf/ctox_perf_probe.py --assert-idle --pretty --cpu-samples 600 --cpu-interval 1 > runtime/build/ctox-idle-10min.json
```

The release gate must fail automatically when CPU, status latency, DB growth,
SQLite/runtime heartbeat deltas, or `rxdb_subjects.lagged_items_total` exceed
the configured idle budget.

## Immediate Implementation Order

1. Run the assert-enabled perf probe against the installed dev daemon and keep
   the JSON artifact with the release evidence.
2. Done on 2026-06-26 for the direct Coding Agents provider status poller:
   the module no longer schedules a 10 second diagnostics interval; keep
   checking other recurring browser command producers.
3. Done on 2026-06-26 for the direct native browser-runtime 300 ms
   active-session DB poller: after one empty maintenance pass it waits on
   `browser_input_events` table changes or the slow idle timeout. Remaining
   work is installed idle evidence and loop counters for input queries, timeout
   wakes, and expired-frame GC.
4. Replace queue/chat aggregate stamps with changed-id/high-water repair
   cursors, and batch active queue repair command/task lookups.
5. Add native SQLite fallback counters and reject/guard large unsupported
   fallback queries.
6. Done on 2026-06-26: implement RxSubject bounded/lagged checkpoint resync
   and expose `rxdb_subjects.lagged_items_total` through peer/perf-probe
   evidence.
7. Make file demand fetch range-aware and add DB growth/retention diagnostics.
8. Finish browser IndexedDB complex-query subscription deltas.
9. Batch browser chunk uploads and add local push scan/fallback counters; the
   immediate local-write trigger burst path is already debounced/coalesced.
10. Work through UI/module, mailserver, execution, inference, and mission
    residuals with targeted perf tests.

## Completion Criteria

This performance review is handled only when all of the following are true:

- every fixed/partial/open row above is either fixed, explicitly deferred with
  owner and release rationale, or covered by a failing guard;
- installed `ctox-real` passes the 10 minute post-file-access idle probe;
- native heartbeat deltas prove no continuous loop work when sources are
  unchanged;
- native and browser RxDB hot queries have bounded row visits;
- file share/update/delete scenarios show bounded DB growth after maintenance;
- release artifacts contain probe JSON, DB diagnostics, commit id, release id,
  and installed binary target.
