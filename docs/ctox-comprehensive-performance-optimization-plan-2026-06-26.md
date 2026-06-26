# CTOX Comprehensive Performance Optimization Plan - 2026-06-26

Source review:
`/Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md`

Workspace:
`/Users/michaelwelsch/Documents/ctox.nosync`

## Verdict

No. The 2026-06-24 performance review is not fully handled.

The current tree contains meaningful fixes for several original hot paths,
especially native RxDB query-fetch, file chunk churn, WebRTC diagnostics, IMAP
body over-fetch, and some status-loop work. It is still not valid to claim that
CTOX is performance-complete or idle-clean.

The remaining high-risk areas are:

- the Coding Agents browser module no longer dispatches recurring provider
  status commands every 10 seconds while open; broader browser command producers
  still need installed idle evidence;
- the native browser runtime loop no longer blind-polls every 300 ms after an
  empty active-session maintenance pass; it backs off to table-change wakeup or
  slow timeout, but still needs installed idle evidence;
- browser Business OS chat tracking DB lookups are now batched, the fixed chat
  scheduler idle timer is removed, and command/queue tracking subscriptions are
  only active while active tracked messages exist; broader Chat DOM/layout and
  shell listener work remains; Chat attachment staging also releases the
  `desktop_file_chunks` bridge after flush;
- several Business OS modules still recompute and re-render on keystroke-scale
  interactions;
- browser IndexedDB `upsert()` no longer has read/write/read overhead, but
  non-indexed `allDocuments()` fallback paths and live-query re-exec remain;
- native RxDB has SQL pushdown for common selectors, but unsupported normal
  `query()` and `count()` paths still fall back to Rust matcher scans;
- native `RxSubject` fanout now uses bounded queues with lagged resync markers;
- Business OS projection loops are much cheaper than before, but still rely on
  periodic source stamps, fallback scans, and some direct projection-writer
  opens;
- local inference and some execution/mission paths from the review remain open;
- installed-daemon idle evidence after `ctox upgrade --dev` is still a release
  gate, not a conclusion.

## Review Method

This plan is based on:

- root guidance in `AGENTS.md`, `README.md`, `HARNESS.md`, and
  `docs/architecture.md`;
- data-plane guidance in `docs/ctox-rxdb.md`,
  `src/core/rxdb/AGENTS.md`, `src/core/business_os/AGENTS.md`, and
  `src/apps/business-os/rxdb/AGENTS.md`;
- direct inspection of the current source with `rg`/targeted reads;
- four read-only subagent reviews:
  - native RxDB/SQLite adapter;
  - Business OS native peer, store, projections, files, and daemon idle loops;
  - browser Business OS/RxDB/WebRTC/module UI paths;
  - mail, inference, execution gateway, mission/report, and service-loop paths.

No subagent edited files. The browser subagent also ran
`node src/apps/business-os/rxdb/tests/run-all.mjs`, reporting 47 passed,
0 failed, and 2 skipped cross-process wire tests because the wire daemon was
not built.

## Review Errata And Status Model

The 2026-06-24 review labels the high section as `HIGH (7)` but names H1-H6.
The count is still useful because H1 was independently confirmed by two audit
passes and is the shared root for native RxDB and async-hygiene findings. This
plan tracks the six named high findings and keeps the duplicated H1/root-cause
weight in the release priority.

Status terms in this plan are intentionally strict:

- `fixed`: the exact reviewed issue is gone in inspected code and has a guard or
  targeted verification.
- `fixed for exact path`: the cited hot path is fixed, but adjacent residual
  work remains and must stay visible.
- `partial`: the issue is reduced, but still has an open architectural,
  verification, or release-evidence gap.
- `open`: the reviewed behavior is still present.

No idle-related finding is release-closed until the installed `ctox upgrade
--dev` daemon passes the P0 idle probes after browser access and file access.

## Current Status Matrix

Legend:

- `fixed`: exact reviewed issue is addressed in inspected code.
- `partial`: reduced, but not structurally closed.
- `open`: still present in inspected code.

| Finding | Status | Current evidence and meaning |
| --- | --- | --- |
| H1 RxDB SQLite non-PK scans | partial | `query()` uses `compile_query_sql()` and pushes common `WHERE`/`LIMIT`/`OFFSET`, but `execute_query_documents()` still has fallback scans for unsupported selectors. |
| H2 WebRTC status per frame | fixed for exact path | Browser status is skinny by default, heavy diagnostics are opt-in, and transport-status emits are throttled/coalesced. Keep observer/fanout counters in the release probe. |
| H3 IMAP FETCH/STORE body overfetch | fixed for exact body path | IMAP server uses message summaries and body-on-demand; large-mailbox sequence resolution still needs SQL pagination and connection reuse work under M17/M19. |
| H4 browser chat tracked-message N+1 | fixed for exact path | `syncTrackedMessages()` now batches tracked command/task reads into one `find()` per collection, coalesces subscription-triggered sync, and command/queue watchers run only while active tracked messages exist. Broader Chat DOM/layout/listener work remains. |
| H5 Matching keystroke recompute | open | Matching still has linear score/search work and undebounced object rendering in inspected UI paths. |
| H6 Outbound per-row pipeline recompute | open | Per-row `pipelineItemForCompany()` still calls `currentPipeline()` and recomputes pipeline state; treat the exact original finding as unfixed. |
| M1 RxDB count materializes docs | partial | SQL counts exist for compilable selectors; unsupported counts fall back to `query()` and are now correctly `slow`, but still scan. |
| M2 single SQLite connection mutex | partial | Many reads use read-only WAL connections, but the shared writer connection remains and some read/status/checkpoint-like paths need audit. |
| M3 query-fetch full scan | fixed | `query_stream` accepts SQL-compilable queries and refuses Rust matcher fallback on the WebRTC query-fetch hot path. |
| M4 projection reconcilers | partial | Queue/chat reconcilers now filter active rows and chat lookups are batched natively; true persisted changed-id/high-water repair windows remain. |
| M5 chunk prune full scan | fixed | Native desktop chunk pruning uses primary-key range bounds. |
| M6 per-chunk write transactions | fixed for native file indexing | Native eager file writes use bulk upsert; browser upload/import chunk flows still need separate lifecycle and batching guards. |
| M7 demand-cache invalidation scans | fixed for exact path | Reverse document-to-window references and batch invalidation exist. Keep behavioral batch tests and idle counters as evidence. |
| M8 browser upsert transaction overhead | fixed | IndexedDB `upsert()` now performs one readwrite transaction, reads the existing row once inside it, writes once, returns the written document, and has a dist-level smoke asserting one transaction/no final re-read. RxDB `bulkUpsert()` now calls storage `bulkUpsert()` once, and storage `bulkUpsert()` batches existing-row reads/writes in one transaction with one coalesced change event. |
| M9 subscriptions full re-query | partial | `collection.$` now keeps an in-memory snapshot and applies change payload deltas without full `find().exec()` after every write. `findOne(primary).$` ignores unrelated changed IDs and applies matching primary-key deltas. Complex query subscriptions still re-query. |
| M10 browser `allDocuments()` fallback | partial | Primary-key/schema-index/bounded cursor paths exist; non-indexed selectors still fall back to `allDocuments()`. |
| M11 inference arena allocation | open | Qwen decode still creates/frees ggml contexts with `mem_buffer: null` per step. |
| M12 inference graph rebuild | open | Qwen graph construction remains per step. |
| M13 streamed event clone/deserialize | fixed for stream delta/no-op path | Direct session now inspects event kind before cloning payloads, drops high-frequency delta/no-op events before deserialization, and has targeted guards for ignored deltas and consumed agent messages. Cost telemetry batching and broader transcript-copy work remain under execution tail work. |
| M14 blocking file-fetch stream | fixed | Native file fetch uses blocking worker plus bounded channel/backpressure instead of parking a tokio worker with `block_on`/`thread::sleep`. |
| M15 unbounded RxSubject fanout | fixed for native RxSubject fanout | Native `RxSubject` now uses a bounded broadcast ring, records lagged items, emits storage lag markers for recoverable change streams, invalidates incremental query buffers on lag, maps storage lag to replication `RESYNC`, surfaces process-wide lag totals in the native peer heartbeat/perf probe, and has targeted slow-peer checkpoint-recovery coverage. Installed/integration slow-peer soak evidence remains. |
| M16 mailbox index | fixed | `stalwart_messages` has mailbox/received index. |
| M17 mailserver connection churn | open | Broad hot-path `with_connection` reuse remains under-specified; write/update/delete paths still open direct connections in inspected paths. |
| M18 send verification full RFC822 | fixed | Send verification uses Message-ID search/header fetch. |
| M19 email full UID scans | partial | UID watermark path exists after a known UID; first/empty sync still uses `UID SEARCH ALL`, with no IDLE/delta-token closure. |
| M20 ticket assignment N+1 | partial | Self-work list/projection hydration now loads latest assignments set-based in one query; single-load and broader ticket/queue bridge paths still need batching. |
| M21 ticket projection DB reopens | fixed for direct projection | Business OS ticket item, event, routing, case, self-work, control-bundle, approval, verification, writeback, and clarification buckets now share one ticket DB connection in the direct projection pass; broader non-projection ticket/queue helpers still need audit. |
| M22 chat full DOM rebuild | partial | Chat has some in-place paths, but no-op syncs can still build message HTML and compare serialized `innerHTML`; signature/append-only reconcile remains. |
| M23 forced reflow in drag | open | Window-manager and chat alignment still contain interleaved geometry reads/writes. |
| M24 sync diagnostics fanout | partial | Diagnostics are throttled/coalesced and skinny by default, but every `transportStatus$` event still enters sanitize/record logic; observer/fanout counters remain required. |
| M25 spreadsheet HyperFormula rebuild | open | Spreadsheet recalc still rebuilds HyperFormula and walks cells. |
| M26 matching requirements scans | open | Requirements rendering still uses full list rebuilds and repeated scans. |
| M27 Buchhaltung joins per render | open | Journal rows still scan line arrays per entry in inspected render paths. |
| M28 customers search full render | open | Account search still renders left plus center instead of center-only. |
| M29 projection writer reopen/table_info | partial | `RxdbProjectionWriterCache` exists for some batch-like paths, and non-command `push_collection_records` batches now use one core-store connection plus one transaction; generic `upsert_rxdb_collection_record()`, command-path batching, and production counters remain. |
| M30 `synchronous=NORMAL` | fixed for checked central stores | `open_store_connection` and core persistence set `synchronous=NORMAL`; future direct SQLite helpers need a guard so the pragma cannot regress. |
| M31 status `ps` scan | fixed for status idle path | `matching_service_processes` is TTL-cached for normal status polling and app recovery is off the UI cadence; explicit lifecycle/probe scans remain intentional. |

## Additional 2026-06-26 Subagent Findings

These findings were not all named precisely in the 2026-06-24 review, but they
are now explicit release blockers for an idle-clean claim:

- Coding Agents idle status loop: done on 2026-06-26 for the direct browser
  poller. `src/apps/business-os/modules/coding-agents/index.js` no longer starts
  the 10 second diagnostics interval while mounted; provider status checks remain
  initial/manual/user-action commands. The module test now asserts no diagnostic
  interval is scheduled and no `setInterval(` remains in the module source.
- Business Chat active collection pinning: done on 2026-06-26 for the direct
  command/queue subscription path. Chat now starts `business_commands` and
  `ctox_queue_tasks` subscriptions plus the 4 second tracking fallback only while
  active tracked messages exist, and stops them when tracking reaches terminal or
  no-tracked state. The shared test asserts terminal/no-tracked Chat state creates
  no subscriptions/timer and that active tracking tears them down after terminal.
- Chunk bridge lifecycle: done on 2026-06-26 for Chat attachments. File-backed
  Chat staging waits for `desktop_file_chunks` flush/in-sync and then stops that
  transient collection bridge; `stopCollection()` now also removes the collection
  from the sync runtime's restart-interest set. Other file-backed browser upload
  surfaces still need the same explicit lifecycle audit.
- Native browser runtime loop: done on 2026-06-26 for the direct 300 ms
  active-session DB polling path. `rxdb_peer.rs` now tracks whether maintenance
  actually processed input/GC work; after one empty round it waits on the
  `browser_input_events` table-change notifier or the slow idle timeout instead
  of reacquiring the DB write lock every 300 ms. Installed daemon evidence is
  still required because other runtime/daemon timers remain open.
- WebRTC handshake checkpoint fanout: rebuilding the native room payload can
  perform per-collection `replication_checkpoint_status()` reads through the
  shared writer lock. Cache/coalesce checkpoint status or move these reads to a
  read-only path.
- Queue reconcile per-row lookups: the active queue repair window still performs
  per-row command/canonical task lookups. Replace with batched lookup maps or a
  persisted changed-id/high-water repair cursor.
- Progressive mission maintenance: when progressive autonomy is enabled, the
  15 second mission maintenance tick can list durable self-work through
  approval auto-close paths. Gate it on table-change/source-change signals.
- Browser active collections remain only partially lifecycle-driven. The active
  collection registry expires stale hints, but module startup can still request
  broad sync collections explicitly; move module sync to demand/subscription
  ownership before claiming browser idle is closed.
- Non-Chat chunk bridges remain partial. Chat attachments now release
  `desktop_file_chunks`, but command bus and CV Print Builder chunk workflows
  still need lease/ref-counted release semantics after flush.
- Startup progress and reporter pointer activity still have idle/listener
  residuals: the progress creep can keep a short interval alive during stalled
  startup, and reporter activity still uses duplicate high-frequency pointer
  listeners.

## Important Low-Finding Buckets Still Open

These are not the first release blockers, but they should not be lost:

- installed/integration slow-peer soak evidence after checkpoint resync recovery
  from `RxSubject` lag;
- browser `encodedSize()` allocations and repeated transfer-size checks;
- frame reassembly `highestContiguousSeq()` recomputing from zero per chunk;
- browser chunk upload batching and local push scan/fallback counters after the
  completed local-write trigger debounce;
- Business OS shell idle layout/listener work after the fixed chat scheduler;
- CV Print Builder, Conversations, Outbound, Matching, Buchhaltung, Customers,
  and Spreadsheet render/reload patterns;
- ticket/queue mission N+1 paths;
- working-hours canonicalization and process-mining authorizer env lookup;
- report `list_runs` redundant Rust sort;
- physical DB growth/backlog explainability for tombstoned or blanked chunk rows.

## Finding Acceptance Matrix

These checks are the minimum evidence needed before a finding can move from
`partial` to release-closed.

| Area | Required evidence |
| --- | --- |
| H1/M1/M3/M10 SQLite and browser query fallback | Hot query tests assert intended indexes with `EXPLAIN QUERY PLAN`; idle probes show `fallback_calls == 0` and no growth in `rows_visited`/`rows_decoded`; browser smokes fail if hot selectors call `allDocuments()`. |
| H2/M24 diagnostics | Counters record `transport-status` emits/sec, heavy diagnostic snapshot builds, and fanout count; with no diagnostics observer, heavy snapshot builds stay at zero. |
| H3/M17/M19 mail | `FETCH FLAGS`/`STORE`/verification fetch zero message bodies; large mailbox sync opens bounded connections, uses UID watermarks after first import, and does not run repeated `UID SEARCH ALL` in steady state. |
| H4/M22/M23 chat and shell | 100 tracked messages perform one command query and one queue query; no-active-tracking chat holds no command/queue subscriptions; no-op chat sync does not serialize full `innerHTML`; drag frames batch layout reads before writes. |
| H5/H6/M25-M28 module UI | Fixture smokes seed at least 1k records per affected module and assert bounded render count, bounded DB calls, debounced search input, and keystroke latency below the chosen UI budget. |
| M2/M15 native concurrency | Writer lock wait/hold p95 is recorded; `RxSubject` uses bounded queues with explicit lagged checkpoint resync and no unbounded memory growth under a stalled subscriber. |
| M5/M6 file/chunk paths | File access/materialize/import scenarios record chunk rows, WAL/freelist sizes, transactions per upload, bytes read vs requested range, and retained bytes after maintenance. |
| M20/M21 ticket projection | Self-work list hydration performs one assignment batch query; the direct projection pass opens one ticket DB connection across all ticket projection buckets; remaining queue bridge hydration is batched by `IN(...)`. |
| M29/M30/M31 store/status | Projection writer bursts open SQLite O(collections), not O(records); `PRAGMA table_info` is cached per collection; status polling p95 stays below 100 ms and 2 Hz polling does not measurably raise daemon CPU. |
| M11-M13 inference/execution | Benchmarks record per-token host time, allocations per token, graph rebuild count, and event clone/deserialize count before and after each change. |

## Optimization Plan

### P0 - Release Evidence Gate

Do this before claiming idle-clean behavior.

Tasks:

1. Run the installed dev build path:
   - push current main-ready fixes;
   - run `ctox upgrade --dev`;
   - record installed binary path, release/build id, git commit, and
     `ctox-real` PID.
2. Run `src/tools/perf/ctox_perf_probe.py --assert-idle` for these scenarios:
   - fresh daemon, no browser;
   - Business OS open and synced;
   - after file access grant;
   - File Viewer materialize/read;
   - CV Print Builder open and idle;
   - 10 minutes no input after warmup.
3. Store JSON artifacts under `runtime/build/perf/` or a release artifact path:
   - CPU average and p95;
   - status latency p95;
   - DB file/WAL/SHM sizes before and after;
   - native peer heartbeat loop deltas;
   - SQLite runtime fallback counters;
   - frame/query/file fetch counters where available.
4. Add a release checklist entry that refuses an idle-clean claim when artifacts
   are missing.
5. Include a short `sample` or `spindump` capture for any scenario that exceeds
   the CPU budget, so the next fix is based on the actual hot stack.

Acceptance:

- `ctox-real` stays below 2 percent average CPU over a 5 minute idle probe.
- 10 minute no-input scenario stays below 5 percent sustained CPU p95.
- `ctox status --json` p95 stays below 100 ms, and 2 Hz status polling does not
  measurably increase daemon CPU.
- No native heartbeat loop counter indicates continuous expensive work while
  source stamps are unchanged.
- No SQLite fallback/scan counter grows continuously while idle.
- No DB/WAL file grows monotonically during idle.

### P1 - Browser Command Producers, Chat Tracking, And Shell Idle

This is the biggest remaining browser high issue.

Tasks:

1. Done on 2026-06-26 for Coding Agents status refresh:
   - removed the unconditional 10 second provider status command interval;
   - kept provider status checks initial/manual/user-action driven;
   - added an idle regression proving the module does not schedule recurring
     diagnostics refreshes.
2. Done on 2026-06-26 for the direct Chat active collection lifecycle:
   - `business_commands` and `ctox_queue_tasks` are no longer subscribed globally
     when there are no active tracked command/task messages;
   - observers and the 4 second tracking fallback stop after tracked messages reach
     terminal state;
   - the shared idle regression covers terminal/no-tracked Chat state and teardown
     after active tracking becomes terminal.
3. Partially done on 2026-06-26 for chunk bridge lifecycle after file-backed
   browser actions:
   - done for Chat attachments: after staging and `awaitInSync`, Chat releases
     `desktop_file_chunks`, and `stopCollection()` now clears restart interest;
   - remaining: audit non-Chat browser upload/import surfaces for the same
     transient chunk-bridge release;
   - remaining: implement a shared lease/ref-counted collection bridge owner so
     command bus, CV Print Builder, and future upload/import surfaces cannot keep
     `desktop_file_chunks` active after flush.
4. Done on 2026-06-26: replace `syncTrackedMessages()` serial
   `findOne().exec()` calls with batched `$in` reads, one for commands and one
   for queue tasks.
5. Done on 2026-06-26: coalesce subscription-triggered syncs so only one run is
   in flight, and disarm the fixed 4 second fallback unless active tracked
   messages exist.
6. Done on 2026-06-26 for the DB-roundtrip regression: the shared JS test seeds
   40 tracked messages and asserts one command query, one queue query, and zero
   `findOne()` fallback calls.
7. Fix chat render:
   - content signature per open chat window;
   - append/reconcile changed nodes;
   - avoid `innerHTML` serialization for no-op syncs.
8. Fix always-on shell timers/listeners:
   - Done on 2026-06-26: chat scheduler arms only when scheduled messages exist
     and clears the previous global 1 second interval.
   - chat alignment uses a single rAF;
   - reporter pointer activity has one throttled listener;
   - startup progress creep stops at cap or uses CSS transition.
9. Move broad `startModuleSync()` calls toward demand/subscription ownership so
   opening a module does not permanently widen active collection sync.

Acceptance:

- 100 tracked messages cause O(1) collection queries, not O(messages).
- Open Coding Agents module causes zero recurring provider status command writes
  while provider status is fresh.
- Open Chat with no active tracked messages does not keep command/queue
  collections active indefinitely.
- File-backed browser upload leaves no active chunk bridge after flush.
- No browser idle timer scans all chats when no scheduled message exists.
- Opening and closing CV Print Builder, File Viewer/materialize, and command-bus
  file workflows leaves no stale `desktop_file_chunks` active collection.
- Performance smoke fails if tracked-message DB calls regress to per-message.

### P2 - Browser IndexedDB Storage And Live Query Architecture

Tasks:

1. Done on 2026-06-26: collapse `storage-indexeddb.upsert()` to one
   readwrite transaction, one existing-row read, one write, and direct return
   of the written document. `storage-index-smoke` now guards the transaction
   and request counts against a read/write/read regression.
2. Done on 2026-06-26: make browser `bulkUpsert()` a real batch path. The RxDB
   facade calls storage `bulkUpsert()` once, storage merges existing documents
   in one readwrite transaction, and the dist-level smoke guards one
   transaction, one lwt-floor cursor, one coalesced change event, and no
   per-document facade `upsert()` calls.
3. Finish indexed query coverage:
   - audit all `allDocuments()` fallbacks under Business OS usage;
   - add schema indexes for hot selectors;
   - refuse or log unsupported large unindexed shapes in dev diagnostics.
4. Partially done on 2026-06-26 for subscription deltas:
   `collection.$` keeps a previous result set and applies changed-id payloads
   in memory, while `findOne(primary).$` only reacts to its primary key.
   Complex selector/sort query subscriptions still need indexed-window or
   explicit full-refresh fallback semantics.
5. Replace static-only demand invalidation checks with behavioral tests that
   count `scanQueryWindows()` calls under large batches.

Acceptance:

- Single-document `upsert()` performs one IndexedDB transaction.
- Live-query updates over large collections do not call `allDocuments()` for
  single-row changes.
- Tests fail on unexpected fallback scans for hot Business OS collections.

### P3 - Native RxDB SQLite Completion

Tasks:

1. Finish planner-first SQLite coverage:
   - expand `compile_query_sql()` for real Business OS selector shapes still
     falling back;
   - integrate the existing RxDB `query_planner` concepts rather than growing
     ad hoc selector handling indefinitely;
   - add query-plan tests for each hot collection/query pair;
   - keep fallback counters and mark fallback counts as `slow`.
2. Define fallback policy:
   - WebRTC query-fetch already rejects unsupported fallback;
   - normal `query()` should either compile, prove bounded, or emit diagnostic
     evidence when it scans.
3. Audit remaining read-like paths:
   - checkpoint/status reads;
   - WebRTC handshake checkpoint payload fanout across collections;
   - cleanup/maintenance reads;
   - any read under shared writer lock.
4. Fix `RxSubject` backpressure:
   - Done on 2026-06-26: native `RxSubject` uses bounded broadcast fanout,
     lag counters, storage lag markers, query-buffer invalidation, and
     replication `RESYNC` mapping;
   - Done on 2026-06-26: native peer performance snapshots expose
     `rxdb_subjects.lagged_items_total`, and `ctox_perf_probe.py --assert-idle`
     fails if that counter grows during the sample;
   - Done on 2026-06-26: targeted slow-peer recovery test proves a lagged
     master-change subscriber receives `RESYNC` and recovers all missed docs
     via `master_changes_since`;
   - remaining: installed/integration slow-peer soak evidence.

Acceptance:

- Idle probes show no growing `query_fallback_rows_visited`.
- Hot query tests verify `EXPLAIN QUERY PLAN` uses intended indexes.
- A stalled WebRTC subscriber cannot accumulate unbounded memory.

### P4 - Business OS Native Projection And File Maintenance

Tasks:

1. Replace table-sized source stamps with change generations:
   - persist per-source high-water/change ids;
   - update on writes;
   - projection loops read only changed rows.
2. Replace aggregate repair summaries with cursor windows:
   - queue projection repair by changed task/command ids;
   - batch active queue repair command/canonical task lookups instead of per-row
     `find_one`/load calls;
   - chat repair by changed command/task/chat ids;
   - no global repair fanout from local reconcile.
3. Move remaining direct projection call sites onto `RxdbProjectionWriterCache`:
   - command acceptance/completion/failure;
   - file/share/release fanout;
   - document/version projections;
   - Business OS control paths.
   - add productive column/statement caching so `PRAGMA table_info` is not paid
     per record on generic writer paths.
4. Convert desktop-file fallback scanning into a safety path:
   - file watcher or dirty-root trigger;
   - periodic fallback only after a long quiet interval;
   - explicit loop metrics for files visited, DB rows touched, and duration.
5. Add physical DB-growth control:
   - count live/deleted/blanked chunk rows;
   - report freelist pages and WAL size;
   - operator-triggered checkpoint/vacuum policy, not hidden idle `VACUUM`.
6. Partially done on 2026-06-26 for native browser-runtime active-session
   polling:
   - the loop now backs off after one empty maintenance pass instead of polling
     SQLite/RxDB every 300 ms while the browser is otherwise idle;
   - pending input events wake the loop through the `browser_input_events`
     table-change notifier, with slow timeout fallback for frame GC;
   - remaining work: expose loop counters for active-session ticks,
     pending-event queries, timeout wakes, and expired-frame GC work, then prove
     the installed daemon stays idle.

Acceptance:

- Projection idle loop performs zero table-sized hash/count work when no source
  changed.
- Active browser session with no input causes no high-frequency DB write-lock
  polling.
- Desktop-file idle loop does not recursively stat the shared tree on every
  short cadence.
- Projection writer burst opens SQLite O(collections), not O(records).
- `PRAGMA table_info` is O(collections) per process/pass and guarded by a test.

### P5 - Business OS Module UI Hot Paths

Tasks:

1. Matching:
   - build `matchesByRequirementId`, `matchesByObjectId`, `objectsById`;
   - cache normalized search haystacks;
   - debounce keystrokes;
   - reconcile DOM instead of full teardown.
2. Outbound:
   - compute `currentPipeline()` and `currentCompanies()` once per render;
   - build `pipelineByCompanyId`;
   - stop full `loadAll()+render()` for unrelated subscription changes.
3. Spreadsheet:
   - persistent HyperFormula engine;
   - `setCellContents()` on changed cells;
   - update only changed cells reported by the engine.
4. Buchhaltung:
   - pre-aggregate journal lines by entry id;
   - cache receipt/customer maps;
   - targeted reloads per changed collection.
5. Customers:
   - debounce account search;
   - center-only list render for search;
   - shared summary/handoff calculations.
6. CV Print Builder and Conversations:
   - delegated row listeners;
   - targeted list updates;
   - data signatures to avoid full reloads.

Acceptance:

- Add representative module perf smokes that seed large fixture data and assert
  bounded render counts, DB calls, and keystroke latency.
- Minimum fixture target for first guards: 1k objects/requirements/matches for
  Matching, 1k companies/pipeline rows for Outbound, 1k journal entries plus
  lines for Buchhaltung, and a multi-sheet spreadsheet with enough formulas to
  catch full-engine rebuilds.
- No module search box performs O(all records) work synchronously on every
  keydown without debounce.

### P6 - Mail, Mission, Execution, And Inference Tail Work

Tasks:

1. Mail:
   - add SQL-paginated sequence/header paths for large IMAP folders so
     sequence resolution does not load every summary;
   - route remaining hot write/update/delete paths through `with_connection`;
   - persist account/folder UID watermarks and handle UIDVALIDITY;
   - consider IMAP IDLE for active mailboxes.
2. Mission/tickets/queue:
   - Partially done on 2026-06-26: latest self-work assignment hydration is
     set-based for list/projection paths, and Business OS ticket item/case/self-
     work projection helpers reuse the projection DB connection;
   - Done on 2026-06-26: control bundles now use `list_control_bundles_on_conn`,
     and the direct Business OS ticket projection pass is guarded to open one
     ticket DB connection across its buckets;
   - remaining: batch-load queue bridge task/ticket rows;
   - remaining: preload spill-candidate bridge/failure signatures;
   - remaining: audit single-load and non-projection ticket APIs for per-row
     assignment hydration or root-based DB reopens.
3. Service low paths:
   - cache canonical working-hours root;
   - capture process-mining authorizer flags once;
   - gate progressive mission approval/self-work maintenance on table-change or
     source-change signals, not a blind 15 second durable query sweep;
   - remove redundant report `rows.sort_by`.
4. Execution gateway:
   - Done on 2026-06-26: direct session inspects event method/payload type
     first and drops high-frequency delta/no-op events before cloning or
     deserializing payloads;
   - buffer cost telemetry to one write per turn where possible.
5. Inference:
   - persistent descriptor arenas or reusable ggml graph contexts;
   - graph/context reuse keyed by decode shape;
   - benchmark before/after per-token host overhead.

Acceptance:

- These paths have targeted unit tests or microbenchmarks where possible.
- No service UI/status cadence does fork/exec or repeated DB open work.
- Inference improvements are gated by per-token host-time benchmarks.
- Ticket projection tests assert one assignment hydration batch call and one
  ticket DB connection per projection pass.

## Validation Matrix

Run the narrowest relevant checks after each slice:

Native RxDB:

```sh
cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml
CARGO_TARGET_DIR=/tmp/ctox-rxdb-target cargo test --manifest-path src/core/rxdb/Cargo.toml
node src/apps/business-os/rxdb/tests/run-all.mjs
```

Business OS native:

```sh
rustfmt --edition 2021 --check src/core/business_os/rxdb_peer.rs src/core/business_os/store.rs
CARGO_TARGET_DIR=/tmp/ctox-business-os-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox reconcile_business_chat_tracking_projections -- --nocapture
CARGO_TARGET_DIR=/tmp/ctox-business-os-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox desktop_file_index -- --nocapture
CARGO_TARGET_DIR=/tmp/ctox-business-os-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox rxdb_projection_writer -- --nocapture
```

Browser RxDB and Business OS shell:

```sh
node src/apps/business-os/rxdb/tests/run-all.mjs
node src/apps/business-os/rxdb/tests/transport-status-throttle-smoke.mjs
node src/apps/business-os/rxdb/tests/sync-diagnostics-throttle-smoke.mjs
```

Browser idle scenarios to add before release:

- Coding Agents module open with fresh provider status: assert zero recurring
  provider status command writes over a multi-minute idle window.
- Chat open with no active tracked messages: assert command/queue active
  collection hints settle or narrow.
- File-backed Chat attachment after flush: covered; other command/upload surfaces
  still need assertions that `desktop_file_chunks` is not kept active by stale
  browser hints.

Installed idle proof:

```sh
ctox upgrade --dev
python3 src/tools/perf/ctox_perf_probe.py --assert-idle --pretty
python3 src/tools/perf/ctox_perf_probe.py --assert-idle --cpu-samples 600 --cpu-interval 1 --pretty
```

## Current Verification

Run while updating this plan and the browser/RxDB hot-path fixes:

- `node src/apps/business-os/rxdb/tests/run-all.mjs`: 47 passed, 0 failed,
  2 skipped because the cross-process wire daemon is not built.
- `npm run test:shared` from `src/apps/business-os`: 57 passed, 0 failed,
  including the Chat active-tracking subscription lifecycle and attachment chunk
  bridge release regressions.
- `node --test src/apps/business-os/shared/business-chat.test.mjs`: 8 passed,
  including batched tracking reads, tracking watcher teardown, scheduler idle
  behavior, and Chat attachment `desktop_file_chunks` release.
- `node src/apps/business-os/rxdb/tests/sync-diagnostics-throttle-smoke.mjs`:
  passed after tightening `stopCollection()` cleanup.
- `node --test src/apps/business-os/modules/coding-agents/tests/coding-agents.test.mjs`:
  10 passed, including the removed diagnostics interval.
- `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox browser_runtime_maintenance_sleep_backs_off_after_idle_round -- --nocapture`:
  1 passed, covering the native browser-runtime idle backoff decision.
- `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox ticket_self_work_list_batches_latest_assignment_hydration -- --nocapture`:
  1 passed, covering set-based latest-assignment hydration for self-work lists.
- `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox business_os_ticket_projection_reuses_one_ticket_db_connection -- --nocapture`:
  1 passed, covering one ticket DB open across the direct Business OS ticket
  projection pass, including control bundles.
- `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox sync_ticket_state_projects_local_ticket_items_and_events -- --nocapture`:
  1 passed, covering the Business OS ticket projection after connection-threaded
  ticket item/case/self-work/control-bundle list helpers.
- `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox direct_session_ignores_stream_delta_events_before_deserialize -- --nocapture`:
  1 passed, covering that high-frequency stream deltas are filtered before
  event-payload deserialization.
- `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox direct_session_extracts_agent_message_events -- --nocapture`:
  1 passed, covering that consumed agent-message events are still parsed.
- `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox direct_session -- --nocapture`:
  20 passed, covering the full direct-session unit-test filter after the event
  hot-path change.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml rxjs_compat::tests -- --nocapture`:
  8 passed, covering bounded `RxSubject` backlog, process-wide lag counters,
  and lag-signal behavior.
- `CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox native_peer_status_reports_fresh_heartbeat -- --nocapture`:
  1 passed, covering that native peer performance status includes the
  `rxdb_subjects.lagged_items_total` release-evidence counter.
- `python3 -m py_compile src/tools/perf/ctox_perf_probe.py`.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml change_event_buffer::tests::lagged_marker_invalidates_incremental_buffer -- --nocapture`:
  1 passed, covering query-buffer invalidation after storage change-stream lag.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml replication_protocol::index_mod::tests::storage_master_change_stream_lag_maps_to_resync -- --nocapture`:
  1 passed, covering storage lag to replication `RESYNC` mapping.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml replication_protocol::index_mod::tests -- --nocapture`:
  4 passed, covering desktop chunk response limiting, storage lag to
  replication `RESYNC`, and slow master-change peer checkpoint recovery after
  lag.
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-m15-target cargo test --manifest-path src/core/rxdb/Cargo.toml -- --test-threads=1 --nocapture`:
  271 unit tests and 30 conformance tests passed.
- `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`.
- `node src/apps/business-os/rxdb/tests/run-all.mjs`: 47 passed, 0 failed,
  2 skipped because the cross-process wire daemon is not built.
- `rustfmt --edition 2021 --check src/core/business_os/rxdb_peer.rs`.
- `rustfmt --edition 2021 --check src/core/mission/tickets.rs`.
- `rustfmt --edition 2021 --check src/core/execution/agent/direct_session.rs`.
- `git diff --check` over the touched docs, browser RxDB, Business Chat, and
  native/browser hot-path files.

Not yet run: installed-daemon idle proof after pushing main and reinstalling with
`ctox upgrade --dev`. That remains the release gate for any idle-clean claim.

## Closure Criteria

This performance review is handled only when all of the following are true:

- every high finding is `fixed`, not `partial`;
- every medium finding is either `fixed` or has a documented reason it is not
  on a user-visible hot path;
- unsupported SQLite/browser query fallbacks are observable and bounded;
- Business OS idle loops have no table-sized work when sources are unchanged;
- open browser modules do not create recurring command/status writes when their
  projected state is fresh;
- active browser sessions do not cause 300 ms native DB polling with no input;
- browser chat/module hot paths have performance smokes, not just correctness
  smokes;
- installed `ctox-real` idle evidence after `ctox upgrade --dev` is stored and
  passes the agreed CPU/status/DB-growth budgets.
