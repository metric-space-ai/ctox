# CTOX Performance Review — 2026-06-24

**Scope:** Full performance audit across CTOX native Rust (excluding the upstream
Codex fork under `src/core/harness/`) and the Business OS browser app.
**Method:** 11 subsystems audited in parallel by reading the real code (grep to
locate, Read the actual regions). Every `high`/`medium` candidate was then
**adversarially verified** by a separate pass that opened the cited code and
tried to refute the claim — ruling out cheap clones, sync-context `fs`, scans
over tiny/bounded tables, and debounced re-renders, and adjusting severity to
what the evidence supports. `low` findings were recorded but not independently
re-verified.
**Result:** 78 candidates → **73 confirmed** (7 high / 31 medium / 35 low),
**5 rejected or downgraded**.

> Reproducibility: multi-agent workflow run `wf_4972e381-c51`
> (79 agents, ~4.35M subagent tokens). Raw structured output:
> `tasks/wxkyoyten.output`.

---

## Executive summary

### The dominant root cause

**The RxDB SQLite backend performs no predicate / index / `LIMIT` pushdown.**
Any query whose selector is not a primary-key equality falls into a full-table
scan: `SELECT data FROM <table>` with no `WHERE`, `serde_json::from_str` on every
row, sort in Rust, and `LIMIT`/`skip` applied *after* the whole table is read.
The required indexes (`lastWriteTime`, `(deleted,lastWriteTime,id)`) exist and a
`query_planner.rs` already reasons about index selection, but the planner is
**not wired into the SQLite backend** (the in-memory backend uses it correctly
via `RxQueryPlan`). This single gap was independently confirmed as `high` by two
finders and is the root of ~12 further findings (`count()`, the projection
reconcilers, the browser query-fetch handler, the command consumer, chunk
pruning, …). It is compounded by a **single `Arc<Mutex<Connection>>` per DB**,
which discards WAL read concurrency — one slow scan blocks the entire sync mesh.

> **Reach caveat (from the verifier):** several hot *native* Business OS
> operations use a separate direct-rusqlite record store rather than
> `RxStorageInstance`, so the worst linear-scan impact concentrates on
> **browser-driven RxDB queries/counts and replication-adjacent reads** routed
> through `RxQuery::exec` — which is exactly the interactive path.

### Secondary themes

- **Diagnostics rebuilt on the hot frame path** — transport-status snapshots are
  rebuilt and broadcast per DataChannel frame (H2, M-`sync.js`), pure overhead
  that throttles real sync throughput.
- **Full re-render + recompute on every keystroke** in Business OS modules — no
  memoization, no `Map` indexing, no debounce, no virtualization (H5, H6, and
  six medium module findings).
- **IMAP/email loads full bodies and full UID lists on hot paths** (H3 + four
  medium communication findings).
- **Inference rebuilds the graph and re-mallocs a ~768 MB arena per token**
  (two medium inference findings).
- **Native N+1 / fresh-DB-open-per-row** in mission/report and the store
  projection writer.

### Severity distribution

| Subsystem            | high | medium | low | total |
|----------------------|:---:|:-----:|:---:|:-----:|
| store-sqlite         | 1\* | 2     | 5   | 7 (\*shared root w/ rxdb-native) |
| service-loop         | 0   | 1     | 5   | 6     |
| rxdb-native          | 1   | 5     | 2   | 8     |
| rxdb-browser         | 1   | 4     | 3   | 8     |
| inference-runtime    | 0   | 2     | 3   | 5     |
| execution-gateway    | 0   | 1     | 4   | 5     |
| async-hygiene        | 1\* | 3     | 1   | 5 (\*same RxDB scan root) |
| communication-native | 1   | 4     | 0   | 5     |
| mission-report       | 0   | 2     | 5   | 7     |
| bos-shell-js         | 1   | 3     | 4   | 8     |
| bos-modules-js       | 2   | 4     | 3   | 9     |
| **Total**            | **7** | **31** | **35** | **73** |

---

## A. HIGH (7)

### H1 — RxDB SQLite: every non-PK query is a full-table scan + per-row JSON deserialize, `LIMIT` not pushed
`src/core/rxdb/src/storage/sqlite/instance.rs:704` (+ `src/core/rxdb/src/storage/sqlite/sql.rs:170`) · `full-scan`
*Independently confirmed as high by two finders (rxdb-native and async-hygiene).*

- **Evidence:** In `RxStorageInstanceSqlite::query`, any query without a
  primary-key equality selector falls into `for_each_document(...)`
  (`instance.rs:709`), which runs `SELECT data FROM <table>` (`sql.rs:175`) and
  `serde_json::from_str` on every row (`sql.rs:180`), then sorts in Rust and
  slices `rows[start..end]` (`instance.rs:716-719`). `limit`/`skip` from the
  MangoQuery are applied after the whole table is read. Indexes on
  `lastWriteTime` and `(deleted,lastWriteTime,id)` exist (`sql.rs:48-51`) but the
  planner is not referenced anywhere under `storage/`. So `{status:{$eq:'x'}}`
  or `{file_id:{$eq:id}}` scans + parses the entire collection.
- **Impact:** O(collection size) CPU + IO + allocation on every
  `find`/`find_one`/`count`/`query` that isn't by primary id. Degrades linearly
  as `business_records`, `ctox_queue_tasks`, `business_commands`,
  `business_chats`, `desktop_file_chunks` grow. Combined with the recurring
  callers below, this is the dominant scaling cost of the data plane.
- **Fix:** Translate single-field equality selectors to `WHERE`, push
  `LIMIT`/`OFFSET` into the prepared statement, use the existing
  `(deleted,lastWriteTime,id)` index for the common non-deleted+recency queries,
  and wire `query_planner.rs` index selection into the SQLite backend.

### H2 — `getTransportStatus()` rebuilds a large snapshot and emits it on every DataChannel frame (send and receive)
`src/apps/business-os/rxdb/src/webrtc-native.mjs:1409` · `render-thrash`

- **Evidence:** `recordTransportStatus(patch)` ends with
  `emit('transport-status', this.getTransportStatus())` (line 1497).
  `getTransportStatus()` builds an object with multiple array spreads/`.map()`
  (`pending`, `observedRequests`, `connections` ×2, `rtcPeerConnectionPoolSnapshot()`,
  `recentRtcEvents.slice`, `recentMessages.slice(-30)`). `recordTransportStatus`
  is invoked on every send, every receive, every chunk ack (line 1011), and every
  chunk-frame send during a transfer (lines 361, 380). Each emission fans out to
  all ~80 collection states (`replication-webrtc.mjs:291, 381-383`).
- **Impact:** A chunked transfer is hundreds of `chunk`+`ack` frames at ≤10 KiB
  each → hundreds of large object allocations + N-collection fanouts per
  transfer, on the main thread, purely for diagnostics — throttling throughput
  and causing GC pressure on the hot data path.
- **Fix:** Update lightweight counters synchronously but debounce/throttle the
  rebuild + emit (≤1/frame or 250 ms). Build the heavy snapshot lazily only when
  a listener actually reads status.

### H3 — IMAP server FETCH/STORE loads every message body in the mailbox per command
`src/core/mailserver/src/imap/mod.rs:234` · `full-scan`

- **Evidence:** The FETCH handler does `get_messages(mailbox_id)` + `.reverse()`
  (`imap/mod.rs:234-236`); STORE is identical (`:359-361`). `get_messages`
  (`store/sqlite.rs:489-527`) runs `SELECT id, ..., body, headers, ... WHERE
  mailbox_id=?1 ORDER BY received_at DESC` — always pulling full `body`+`headers`
  for every message, even when the client asked only for `FLAGS` or STORE needs
  just the id for a sequence number.
- **Impact:** A 5k-message inbox with multi-KB bodies → tens of MB copied out of
  SQLite per command, turning an O(1) flag fetch into O(mailbox size) and an
  N-message sync into O(N × mailbox size).
- **Fix:** Push projection + pagination into SQL: `get_message_headers(mailbox_id)`
  (id/flags/size/received_at only) for sequence resolution; load a single body
  only when `BODY[]`/`RFC822`/`BODY[TEXT]` is actually requested.

### H4 — `syncTrackedMessages` issues serial N+1 RxDB `findOne` queries on every command/queue change and every 4 s
`src/apps/business-os/shared/business-chat.js:2104` · `sqlite-n+1`

- **Evidence:** Nested `for (chat of state.chats) for (message of chat.messages)`
  with up to two sequential `await collection.findOne(id).exec()` per tracked
  message. Driven by `business_commands.$.subscribe(sync)` +
  `ctox_queue_tasks.$.subscribe(sync)` + `setInterval(sync, 4000)`.
- **Impact:** Each sync serializes O(tracked messages) DB round-trips on the main
  thread before any render; the subscription fires repeatedly during active task
  processing → dozens of serialized IndexedDB reads per change event.
- **Fix:** Batch lookups into one `find({selector:{id:{$in:[...]}}}).exec()` per
  collection, map in memory; debounce/coalesce the subscription-driven `sync`.

### H5 — Matching `renderObjects` recomputes O(objects×requirements×matches) scores + a deep search walk on every keystroke
`src/apps/business-os/modules/matching/ui/index.js:7366` · `sqlite-n+1`

- **Evidence:** The `objectSearch` input handler (line 8564) calls
  `renderObjects()` undebounced. It maps every object through `scoreOf` →
  `preMatchScore` → `matches.find(...)` (linear scan per requirement×object pair),
  then `matchesFullTextSearch(buildObjectSearchPayload(c), q)` which rebuilds the
  haystack via `collectSearchText` (recursive NFD-normalize + regex over the whole
  serialized object) fresh on every keystroke.
- **Impact:** Typing in object search is O(objects × requirements × matches) plus
  a full recursive serialize+normalize per keystroke — janky and CPU-bound per
  character at realistic ATS volumes.
- **Fix:** Index matches once into `Map<${requirementId}|${objectId}>` (O(1)
  score); precompute/cache each object's normalized haystack (invalidate on
  reload); debounce input (~120 ms); recompute scores only when data version
  changes.

### H6 — Outbound CRM table: `currentPipeline()` re-dedupes the whole pipeline once per company row
`src/apps/business-os/modules/outbound/index.js:4210` · `sqlite-n+1`

- **Evidence:** `filteredQualificationRows` maps every company to
  `pipelineItemForCompany(company)`, which calls `currentPipeline().find(...)`;
  `currentPipeline()` = `dedupePipelineItems(campaignScopedRows(...).pipeline)`
  (full-array filters + fresh `Map` build) — none memoized, so it re-runs per
  company.
- **Impact:** O(companies × pipeline) on every center render (campaign select,
  filter change, debounced search). Large per-render cost with hundreds of
  companies/pipeline items.
- **Fix:** Compute `currentPipeline()`/`currentCompanies()` once per render (or
  memoize keyed by `selectedCampaignId` + data version); build a
  `pipelineByCompanyId` Map for O(1) lookup.

---

## B. MEDIUM (31)

### RxDB backend & sync (the H1 family)

**M1 — `count()` materializes + deserializes every matching document just to take `.len()`**
`src/core/rxdb/src/storage/sqlite/instance.rs:726` · `full-scan`
Delegates to `query` (full scan) then `documents.len()`; mode is even labeled
"fast". Any UI badge/pagination/policy count pays full materialization.
**Fix:** `SELECT COUNT(*) FROM <table>` + WHERE from the selector; scan-fallback
only for non-expressible selectors.

**M2 — Single `Arc<Mutex<Connection>>` per DB serializes the entire data plane and defeats WAL read concurrency**
`src/core/rxdb/src/storage/sqlite/types.rs:36` · `lock-contention`
One connection per DB file shared by every collection; every read and write
across all ~30+ collections funnels through one critical section. A single slow
scan or a large `bulk_write` blocks every other peer.
**Fix:** Separate read/write connections — keep one `Mutex<Connection>` for
writes, open per-reader read-only connections (the code already does this for
`query_stream`; generalize it). With WAL, multiple readers run concurrently with
the single writer.

**M3 — Browser query-fetch handler full-scans the collection for every list/filter query**
`src/core/rxdb/src/storage/sqlite/instance.rs:906` · `full-scan`
`rxdb.query.fetch` → `query_stream`; even the bounded top-K branch does
`for_each_document` over the whole table with per-row `from_str`. (Memory is
correctly bounded via a dedicated read-only WAL connection — the scan is the
issue.)
**Fix:** Translate indexable selector predicates + limit/skip into the prepared
SQL cursor.

**M4 — Projection reconcilers full-scan `ctox_queue_tasks` and `business_chats` every active cycle (3 s)**
`src/core/business_os/rxdb_peer.rs:4613` · `full-scan`
`reconcile_ctox_queue_task_projections` (`find(limit:500)`, no selector) and
`reconcile_business_chat_tracking_projections` (`find(limit:200)`, no selector)
run on the 3 s projection loop; LIMIT never reaches SQLite.
**Fix:** Track a high-water `updated_at_ms` per collection (the record projection
already does via `since_by_collection`) and pull only changed rows via
`get_changed_documents_since`.

**M5 — `prune_desktop_file_chunk_generations` full-scans `desktop_file_chunks` on every file content upload**
`src/core/business_os/rxdb_peer.rs:6821` · `full-scan`
Selector on `file_id` (not the PK `{file_id}_{generation}_{idx}`) → full-scan +
deserialize over the largest-by-bytes collection on every write.
**Fix:** Read by PK prefix range `id BETWEEN lower AND upper` (bounds already
computed by `desktop_file_chunk_id_bounds`).

**M6 — Desktop file chunks are written one-incremental-upsert-per-chunk (N locked transactions per file)**
`src/core/business_os/rxdb_peer.rs:6451` · `sqlite-n+1`
Each awaited `incremental_upsert` opens its own `Immediate` transaction and
takes the connection mutex; a K-chunk file = K separate locked transactions.
**Fix:** Build all chunk rows and write via one `bulk_upsert`/`bulk_write`.

### RxDB browser

**M7 — Demand-cache invalidation does a full `getAll()` scan of the sidecar window store per batch — twice**
`src/apps/business-os/rxdb/src/query-demand-loader.mjs:168` · `full-scan`
`invalidateDocumentChange` calls `scanQueryWindows()` (bare `getAll()`, no key
range) and JS-filters every cached window; `invalidateDemandCacheForRemoteWrite`
runs twice per batch (`replication-webrtc.mjs:928,932` + `1078,1082`). 1000
changed docs at batchSize 10 ≈ 200 full window scans.
**Fix:** Call once per batch; replace `getAll()`+filter with a reverse
`Map<docId, Set<windowKey>>` so invalidation is O(changed ids).

**M8 — Single-document upsert opens three IndexedDB transactions + a redundant per-row `get` inside `bulkWrite`**
`src/apps/business-os/rxdb/src/storage-indexeddb.mjs:49` · `sqlite-n+1`
`upsert` = read `findOne` + `bulkWrite` (which re-`get`s the same id) + a third
read `findOne`. Every `incrementalPatch`/`remove` routes through `upsert`.
**Fix:** Pass the already-fetched `previous` into `bulkWrite`; return the written
doc directly; collapse to a single readwrite transaction.

**M9 — Collection/query subscriptions re-run the full `find().exec()` on every change instead of applying the delta**
`src/apps/business-os/rxdb/src/rx-database.mjs:222` · `no-virtualization`
`collection.$.subscribe` re-runs `this.find().exec()` (no limit) on every change;
non-fast-path queries fall back to `allDocuments()`. The change event carries the
changed docs (`success`) but discards them.
**Fix:** Diff against the prior result using the changed-id payload; re-query only
when unavoidable, and on an index path.

**M10 — `queryDocuments` falls back to a full `allDocuments()` scan whenever the query isn't the narrow lwt-sorted fast path**
`src/apps/business-os/rxdb/src/storage-indexeddb.mjs:172` · `full-scan`
Indexed path requires a finite limit AND a descending `updated_at_ms`-first sort;
any selector-based query / other sort / non-PK `findOne` scans the whole
collection into memory and filters in JS, though `indexValues`/`selectBestIndex`
exist but are never consulted. `count()` (`rx-database.mjs:182`) is worse —
`(await find().exec()).length`.
**Fix:** Drive a range cursor via `selectBestIndex`/`indexValues`; implement
`count()` as a cursor count.

### Inference runtime

**M11 — Per-token decode re-mallocs ~768 MB of ggml descriptor arena every step**
`src/core/inference/models/qwen35_27b_q4km_dflash/src/driver.rs:179` · `allocation`
Each token does `ggml_free` + `ggml_init` with `mem_size` 512 MB (target) + 256 MB
(draft); `no_alloc` governs tensor data, not the descriptor arena, so the full
arena is malloc'd/freed per step.
**Fix:** Own a long-lived `mem_buffer` of worst-case size (or a persistent
`ggml_context` per graph kind), reset per step rather than free+init.

**M12 — Full 64-layer ggml graph is reconstructed from scratch every decode step**
`src/core/inference/models/qwen35_27b_q4km_dflash/src/graph.rs:1169` · `kernel-dispatch`
`build_qwen35_graph` recreates ~64 layers of nodes + `ggml_build_forward_expand`
+ a full `gallocr` re-plan per token, all host CPU before each GPU submit.
**Fix:** Build the graph + `ggml_gallocr_reserve` once for the fixed decode shape;
re-run alloc only when the DDTree node count changes.

### Execution gateway

**M13 — Every streamed delta event is fully cloned and JSON-deserialized, even though deltas are discarded**
`src/core/execution/agent/direct_session.rs:1582` · `allocation`
`try_extract_event_msg` clones `notif.params`, clones the inner `msg` object
again, then `serde_json::from_value` into an `EventMsg` for every delta — which
then hits the `_ =>` no-op arm. Highest-frequency alloc path in the layer; runs
for the whole duration of every API and local turn.
**Fix:** Inspect `notif.method` first and early-return before any clone for
delta/no-op variants; clone+deserialize only for the consumed events; take
`notif.params` instead of cloning.

### Async hygiene

**M14 — File-stream closure parks a tokio worker for the whole transfer via `futures::executor::block_on` + `std::thread::sleep`**
`src/core/rxdb/src/plugins/replication_webrtc/file_fetch_handler.rs:323` · `blocking-io`
`run_file_fetch` is `tokio::spawn`ed; inside, each chunk send is
`futures::executor::block_on(...)` and backpressure is `std::thread::sleep(8ms)`.
`block_on` (unlike `block_in_place`) does not offload — it holds the worker. The
file's own comment even prescribes `block_in_place`; `rxdb_peer.rs:6669` does it
right.
**Fix:** Wrap in `tokio::task::block_in_place`, or restructure to `.await`
`send_file_chunk` with `tokio::time::sleep`/buffered-amount backpressure.

**M15 — RxSubject change-event fan-out uses per-subscriber unbounded mpsc with no backpressure**
`src/core/rxdb/src/rxjs_compat.rs:76` · `no-backpressure`
`RxSubject` is the change-event backbone (fires on every document write); a
momentarily-slow consumer (stalled WebRTC during an initial-sync burst) grows the
queue unbounded — bounded only by RAM. The unbounded choice is a deliberate trade
against a prior silent-drop bug, but moves the failure from data-loss to
memory-blowup.
**Fix:** Bounded channel with explicit overflow ("lagged, resync from checkpoint")
for high-volume change subscribers; keep unbounded only for low-volume control
subjects.

### Communication

**M16 — `stalwart_messages` has no index on `mailbox_id`, so every mailbox read is a full table scan**
`src/core/mailserver/src/store/sqlite_schema.rs:83` · `missing-index`
The only index is `stalwart_smtp_delivery_log_id_idx`; every `get_messages`
(`WHERE mailbox_id=?1 ORDER BY received_at DESC`) scans the whole table across
all mailboxes/users.
**Fix:** `CREATE INDEX idx_stalwart_messages_mailbox_received ON stalwart_messages(mailbox_id, received_at DESC);`

**M17 — Mailserver store opens a fresh SQLite connection (and runs PRAGMAs) on every hot-path call**
`src/core/mailserver/src/store/sqlite.rs:489` · `allocation`
`get_messages`/`put_message`/`update_message_flags`/`delete_message`/… each call
`connect()` → `Connection::open` + WAL/synchronous PRAGMA batch. A thread-local
`with_connection` cache exists but the message/mailbox hot-path methods don't use
it.
**Fix:** Route message/mailbox methods through `with_connection`; PRAGMAs once at
open.

**M18 — Send-verification re-fetches full RFC822 bodies of up to 25 messages across up to 30 polling attempts**
`src/core/communication/email_native.rs:1210` · `blocking-io`
`verify_imap_inbox_delivery` loops `for attempt in 0..attempts(≤30)` ×
`latest_imap_uids(..., 25)` × `fetch_raw` (full `RFC822`) + parse, just to read a
`Message-ID`. Worst case ≈ 750 full-body fetches+parses while blocking the send.
**Fix:** `UID SEARCH HEADER Message-ID "<id>"` or `BODY.PEEK[HEADER.FIELDS
(MESSAGE-ID)]` for the newest UIDs; stop re-fetching the same UID set every
attempt.

**M19 — Email sync issues `UID SEARCH ALL` every cycle, transferring the whole-mailbox UID list to keep only the newest N**
`src/core/communication/email_native.rs:509` · `full-scan`
`execute_sync` → `search_all_uids()` (full UID set) → sort + `truncate(20)`, every
60 s, no IMAP IDLE.
**Fix:** Track the highest synced UID, use `UID SEARCH UID <last+1>:*` (or
`SINCE`/`UNSEEN`), persist a per-(account,folder) watermark; consider IDLE.

### Mission / report

**M20 — Ticket work-item listing issues an extra assignment query per item (N+1) feeding the Business OS sync projection**
`src/core/mission/tickets.rs:3386` · `sqlite-n+1`
`hydrate_ticket_self_work_item` runs `SELECT ... WHERE work_id=?1 ORDER BY
created_at DESC LIMIT 1` per item (statement re-prepared each call). Feeds
`business_os_ticket_projection_documents` → the periodic RxDB sync.
**Fix:** One windowed query (`ROW_NUMBER() OVER (PARTITION BY work_id ...)` or
correlated `MAX(created_at)`) for all `work_id`s; attach in memory.

**M21 — Business OS ticket projection re-opens the same SQLite DB 5+ times per pass**
`src/core/mission/tickets.rs:7357` · `blocking-io`
`business_os_ticket_projection_documents` opens the ticket DB, then calls
`list_tickets`/`list_cases`/`list_self_work_items`/`list_control_bundles` — each
re-opens the same DB (the other helpers correctly reuse `&conn`). Runs on the
periodic sync, so every ticket data change pays it.
**Fix:** Thread `&conn` into the four list helpers (add `*_on_conn` variants).

### Business OS shell

**M22 — Chat render rebuilds the full messages HTML string and compares against serialized `innerHTML` on every sync**
`src/apps/business-os/shared/business-chat.js:583` · `serialization`
Per open window: `chat.messages.map(messageMarkup).join('')` then reads
`messagesContainer.innerHTML` (forces full subtree serialization) to diff —
O(messages) + O(DOM) per sync tick even when nothing changed; on mismatch a full
reparse.
**Fix:** Track a cheap content signature (last id + count + status hash); skip
when unchanged; append only new nodes.

**M23 — Window drag rAF interleaves `offset*` reads, style writes, then `getBoundingClientRect` reads (forced reflow per frame)**
`src/apps/business-os/shared/window-manager.js:495` · `reflow`
`update()` reads `offsetTop/Left/Width`, writes `style.top/left`, then
`applySnapPreview`/`updateDynamicShadow` read `getBoundingClientRect` — forcing a
synchronous layout flush mid-frame, every frame, while dragging.
**Fix:** Read all geometry once at the top of the rAF, compute, then batch all
style writes with no interleaved reads; cache the surface rect for the drag.

**M24 — Every WebRTC `transportStatus` emission rebuilds a ~60-field object and clones the full per-collection diagnostics map**
`src/apps/business-os/shared/sync.js:652` · `allocation`
`transportStatus$.subscribe(recordTransportStatus)` →
`sanitizeReplicationTransportStatus` (slice/map) → `snapshotDiagnostics` (spreads
the entire collections map, ~80 entries) → always dispatches a window
`CustomEvent`. Steady main-thread alloc/GC under active sync.
**Fix:** Throttle/coalesce to a few Hz; gate the snapshot+dispatch on an actual
observer (the diagnostics drawer is already gated — gate this the same way).

### Business OS modules

**M25 — Spreadsheet recalc rebuilds the entire HyperFormula engine and walks every cell on each edit**
`src/apps/business-os/modules/spreadsheets/index.js:959` · `full-scan`
`onchange` → `HyperFormula.buildFromArray(rawData)` rebuilds the whole engine per
cell edit, then a nested r×c loop calls `getCell` (DOM lookup) per cell.
**Fix:** Build the HF instance once per load; mutate via `setCellContents` on the
changed cell(s); recompute only HF-reported changed cells; drop the full DOM
walk.

**M26 — `renderRequirements` full `innerHTML` rebuild with per-requirement matches scan and linear `getObject` lookups**
`src/apps/business-os/modules/matching/ui/index.js:5499` · `sqlite-n+1`
Undebounced; per requirement `matches.filter(...)` + per-pair `getObject(cid)`
(`objects.find` linear scan) + `matches.find(...)`; full list DOM torn down and
rebuilt.
**Fix:** Build `matchesByRequirementId` + `objectsById` Maps once per load;
replace scans with lookups; debounce; reconcile DOM instead of full teardown.

**M27 — Buchhaltung journal rows: O(entries×lines) line-sum join rebuilt on every render and keystroke**
`src/apps/business-os/modules/buchhaltung/index.js:1000` · `sqlite-n+1`
Per entry `journalEntryLines.filter(...)` (full scan) + `receipts.find(...)`, into
one big innerHTML string; journal search input recomputes per keystroke; any of 5
collections changing triggers a full `loadAllFibuData` reload.
**Fix:** Pre-aggregate line totals into `Map(entryId → totalDebitCents)` and a
`receiptsById` Map once after load; debounce search; avoid full reload on
unrelated single-collection changes.

**M28 — Customers search keystroke triggers a full `innerHTML` re-render of two panes with redundant dataset summaries**
`src/apps/business-os/modules/customers/index.js:1250` · `render-thrash`
Account search input calls `renderCenter()` AND `renderLeft()` per keystroke, both
recomputing `summarizeCustomersData` and building the outbound-handoff index
(twice total), re-serializing two full panes — though the query only affects the
center list.
**Fix:** Debounce; re-render only the center list on search; compute the summary
+ handoff rows once per render and share between panes.

### Store & service-loop (native SQLite)

**M29 — `upsert_rxdb_collection_record` reopens the RxDB DB and re-runs uncached `PRAGMA table_info` up to 4× on every projection write**
`src/core/business_os/store.rs:13196` · `sqlite-n+1`
`Connection::open(&path)` per call, then `rxdb_table_has_column` runs `PRAGMA
table_info(table)` for `deleted`/`_deleted`, `revision`/`_rev`, `lastWriteTime`
(~5 introspections), plus a `SELECT ... WHERE id=?` read-modify-write. This is the
per-record projection writer invoked all over the hot path (15+ call sites incl.
per command/queue-task status change). Schema is static at runtime yet
reflected every time.
**Fix:** Cache per-table column presence in a `OnceLock`/`Mutex` map (like
`RXDB_TABLE_NAMES_CACHE`); reuse a long-lived RxDB connection; build the INSERT
column/placeholder set once per table.

**M30 — Business OS store never sets `PRAGMA synchronous=NORMAL`, so every autocommit write does a full fsync**
`src/core/business_os/store.rs:697` · `missing-pragma`
Only `journal_mode=WAL` + `busy_timeout` are configured; SQLite defaults to
`synchronous=FULL`, which fsyncs the WAL on every commit. Most write helpers
(`upsert_business_record`, the push loop, importer upserts) run as individual
autocommit statements, so every row is its own fsync'd transaction — especially
costly on the iCloud/Documents-backed filesystem.
**Fix:** Add `PRAGMA synchronous=NORMAL;` to the pragma batch in
`open_store_connection` (`store.rs:696`) and `open_sqlite` (`persistence.rs:197`).
NORMAL+WAL is crash-safe for the DB (only risks the last commit on power loss).

**M31 — Status IPC spawns `ps -axo` + scans `/proc` every 500 ms poll on the serial control plane**
`src/core/service/service.rs:3461` · `blocking-io`
`runtime_lifecycle_alerts` (called unconditionally from `status_from_shared_state`)
runs `Command::new("ps").args(["-axo", ...])` plus per-pid `/proc/<pid>/cwd` +
`/proc/<pid>/environ` `canonicalize`. The TUI polls every 500 ms against a 250 ms
IPC budget, and the IPC accept loop is single-threaded/serial — so this fork+exec
+ process-table parse ~2×/sec blocks every other control-plane request and can
force a degraded snapshot. The neighbouring systemd probe was already cached
(5 s TTL) for exactly this reason.
**Fix:** Cache `runtime_lifecycle_alerts`/`matching_service_processes` behind a
short TTL (≈5 s, same pattern as `systemd_unit_status_cached`), or run the scan
only on an explicit lifecycle probe — not on every UI-cadence status poll.

---

## C. LOW (35)

> Recorded but not independently re-verified. Mostly "fresh DB connection instead
> of cache", redundant scans/re-sorts, and idle timer/listener overhead.

### store-sqlite
- **`push_collection_records` opens a new SQLite connection per document** —
  `src/core/business_os/store.rs:13624`. The primary browser→daemon write path;
  100-record sync = 100 open/configure/close + 100 fsync'd transactions.
  *Fix:* open once before the loop, wrap the batch in one transaction.
- **`complete_ready_documents_report_commands` full-scans the unindexed
  `business_commands` every service tick** — `store.rs:12221`. The command log
  grows unbounded; scan paid every tick. *Fix:* index
  `(module, command_type, status, observed_at_ms)`.
- **`find_queue_task_for_command` loads 256 queue tasks and substring-scans per
  command** — `store.rs:26556`. O(256 × promptLen) per status transition.
  *Fix:* resolve by an indexed `command_id`.
- **`invoices_list_due_invoices` full-scans `accounting_invoices` with a
  leading-wildcard `LIKE` then re-parses every match in Rust** —
  `src/core/business_os/invoices.rs:1804`. *Fix:* persist `state`/`open_cents`/
  `due_date_ms` as indexed columns (or `json_extract` expr index).
- **`pull_collection_record` fetches up to 2000 docs then linear-scans in Rust
  for one record id (communication_* collections)** — `store.rs:12512`.
  *Fix:* keyed lookup by id (mirror the `business_records` `query_row` branch).

### service-loop
- **Status poll opens a fresh SQLite connection (LcmEngine) just to read one
  row** — `src/core/service/service.rs:3050`. Every 500 ms per attached TUI.
  *Fix:* long-lived read connection or cache keyed on `core_db_change_stamp`.
- **`count_queue_tasks` opens a fresh DB connection with no cache, twice per
  status poll** — `src/core/mission/channels.rs:2631`. *Fix:* cache behind
  `queue_task_list_cache_stamp` or derive from the cached list.
- **Business OS app-recovery scan runs on every idle status poll with zero
  minimum lease age** — `service.rs:3107`. Per-task artifact-dir FS scan every
  500 ms. *Fix:* minimum lease age + change-stamp gate.
- **SQLite authorizer allocates a `String` via `std::env::var` on every column
  read** — `src/core/service/process_mining.rs:1825`. *Fix:* resolve the bool
  once at attach time into the closure.
- **`working_hours` snapshot calls `fs::canonicalize` on the cache-hit path every
  dispatcher tick** — `src/core/service/working_hours.rs:145`. *Fix:* canonicalize
  the root once at startup.

### rxdb-native
- **Business command consumer full-scans `business_commands` by status every
  ~1 s** — `src/core/business_os/rxdb_peer.rs:2590`. Cost grows with total command
  count, not pending count. *Fix:* push `status='pending_sync'` + `LIMIT` into SQL
  (status index → O(pending)).
- **`bulk_write` does a per-id point query for current DB state instead of one
  `IN(...)` read** — `src/core/rxdb/src/storage/sqlite/instance.rs:564`. N
  single-row queries under the write lock per batch. *Fix:* one `WHERE id IN(...)`.

### rxdb-browser
- **`encodedSize()` allocates a `TextEncoder` and fully UTF-8-encodes the payload
  just to count bytes, per frame send/receive** —
  `src/apps/business-os/rxdb/src/webrtc-native.mjs:2096`. The reassembly path
  encodes the whole (≤8 MiB) transfer twice. *Fix:* reuse a module-level encoder;
  count bytes without materializing; reuse the length on reassembly.
- **Local writes push immediately with a 50× scan multiplier and no debounce on
  the change subscription** — `src/apps/business-os/rxdb/src/replication-webrtc.mjs:721`.
  Floor of 500 scanned index entries per push even for tiny deltas. *Fix:* debounce
  the push-triggering `observe`; make the floor proportional to backlog.
- **Chunk reassembly recomputes highest-contiguous-sequence from index 0 on every
  arriving chunk (O(n²) over a transfer)** — `webrtc-native.mjs:1116`. ~320k
  redundant Map lookups for an 8 MiB transfer. *Fix:* track `entry.contiguousSeq`
  incrementally.

### inference-runtime
- **Per-op Metal dispatch builds a `String` pipeline key and does a mutex-locked
  linear scan** — `src/core/inference/models/qwen35_35b_a3b_dflash/src/metal/ffi.rs:123`.
  layers×ops dispatches each allocate + lock. *Fix:* key PSO lookups by a
  precomputed enum/int id in a lock-free table.
- **Host argmax over the full 248K vocab per slot, serialized after each GPU
  compute** — `qwen35_27b_q4km_dflash/src/driver.rs:502`. Per token, after a
  device→host logits copy. *Fix:* on-device argmax (`GGML_OP_ARGMAX`) or a
  SIMD-friendly argmax.
- **CPU token-embedding dequant runs every decode step and blocks the draft
  submit** — `qwen35_27b_q4km_dflash/src/loader.rs:59`. *Fix:* keep the quantized
  embedding table device-resident, do the lookup as a `ggml_get_rows` node.

### execution-gateway
- **API cost recording opens a fresh SQLite connection and INSERTs per TokenCount
  streaming event** — `src/core/execution/agent/direct_session.rs:1105`. *Fix:*
  accumulate in memory, write one cost row at turn completion.
- **Exact-token `/tokenize` preflight runs a blocking HTTP round-trip twice per
  turn on the same text** — `direct_session.rs:932`. *Fix:* compute once in the
  turn-loop preflight, pass the result into `run_turn_async`.
- **Main runtime-env entry points bypass the existing stamp-validated cache,
  re-opening SQLite each call** — `src/core/execution/models/runtime_env.rs:102`.
  *Fix:* route `load_runtime_env_map`/`effective_operator_env_map`/
  `load_runtime_env_map_for_resolution` through the cached path.
- **Each provider adapter fully parses, clones, and re-serializes the entire
  transcript per upstream round-trip** —
  `src/core/execution/models/model_adapters/anthropic.rs:52`. *Fix:* borrow/iterate
  in place for the already-array case; move items out instead of cloning.

### async-hygiene
- **`collection_checkpoints_payload` awaits checkpoint status sequentially for
  every collection on each cache-miss build** —
  `src/core/rxdb/src/plugins/replication_webrtc/index_mod.rs:1186`. N sequential
  `spawn_blocking` round-trips on a cold build. *Fix:* `join_all`/`buffer_unordered`
  to overlap dispatch. (Note: the entire non-harness core currently has zero
  `join_all`/`buffer_unordered`/`FuturesUnordered` call sites — multi-item async
  IO is uniformly sequential.)

### mission-report
- **Spill-candidate scoring opens a fresh DB and runs `COUNT(DISTINCT blob)` per
  task over up to 10k tasks** — `src/core/mission/queue.rs:2117`. *Fix:* open each
  DB once; replace per-task lookups with two set-based `IN(...)` queries.
- **`emit_due_steps` re-opens the plan DB per due goal and re-scans after every
  step completion** — `src/core/mission/plan.rs:379`. *Fix:* pass one open
  connection through the batch.
- **`list_queue_ticket_bridges` re-opens two DBs per bridge row to hydrate task +
  ticket** — `queue.rs:2099`. 2N opens + 2N point queries. *Fix:* open once;
  batch-load by `IN(...)`.
- **`cleanup_queue_scope` re-queries `metadata_json` per task although the list
  already selected it** — `queue.rs:593`. *Fix:* surface parsed metadata on the
  list view.
- **`list_runs` re-sorts in Rust after the SQL `ORDER BY` already sorted** —
  `src/core/report/runs.rs:133`. *Fix:* delete the trailing `rows.sort_by`.

### bos-shell-js
- **Chat scheduler runs a 1 s `setInterval` forever, scanning DOM and all chats
  even when idle** — `src/apps/business-os/shared/business-chat.js:5001`. Violates
  the OS-snappy/idle-is-idle invariant. *Fix:* arm only when a chat has a
  scheduled message/countdown; clear otherwise.
- **Chat scroll/resize/drag handlers call `alignChatWindows`
  (`getBoundingClientRect` read + writes) with no throttle/rAF** —
  `business-chat.js:181`. Reflow thrash during interaction. *Fix:* coalesce behind
  one rAF; debounce resize.
- **Reporter idle-watcher does DOM `closest()` work on every `mousemove` AND
  `pointermove` (duplicate high-frequency listeners)** —
  `src/apps/business-os/shared/business-reporter.js:329`. *Fix:* drop `mousemove`;
  throttle the activity handler with a timestamp guard.
- **Startup progress bar runs a 16 ms (60fps) `setInterval` that creeps
  indefinitely while target stays below 95%** — `src/apps/business-os/app.js:7897`.
  A stalled boot phase keeps a 60fps style-write loop running. *Fix:* stop the
  creep near its cap, or drive the bar with a CSS transition.

### bos-modules-js
- **cv-print-builder rebuilds list `innerHTML` and re-binds per-row listeners on
  every search keystroke** — `src/apps/business-os/modules/cv-print-builder/index.js:348`.
  *Fix:* one delegated click listener; debounce; reconcile rows.
- **Conversations: any message change reloads all messages and rebuilds the full
  thread list** — `src/apps/business-os/modules/conversations/index.js:406`.
  *Fix:* gate reload on visible-bucket relevance (the check already exists below);
  debounce; reconcile.
- **Outbound realtime: 16 collection subscriptions all funnel into a full
  `loadAll` + render** — `src/apps/business-os/modules/outbound/index.js:1744`.
  *Fix:* scope subscriptions to view-driving collections; targeted partial
  renders; coalesce + skip when the data signature is unchanged.

---

## D. Rejected / downgraded (5) — transparency

These were proposed by finders but **refuted on inspection** by the verification
pass (kept here so the negative results aren't lost):

1. **ggml `gallocr_alloc_graph` "re-plans every step instead of reserve-once"** —
   `driver.rs:252`. *Refuted:* `ggml_gallocr_reserve` only pre-sizes the buffer; you
   must still call `alloc_graph` every step (it isn't even bound in the FFI). The
   decode shape genuinely changes every step (speculative decoding: `q_len`,
   `commit_n`, `kv_pad` vary), and `alloc_graph` internally short-circuits when the
   topology is unchanged. The *graph-rebuild* cost is real (that's M12); the
   `alloc_graph` call specifically is correct.
2. **ContextLogger "unbuffered file syscall per streamed token-count event"** —
   `direct_session.rs:1652`. *Refuted:* TokenCount is emitted ~1–3× per model
   response (per sampling-request / turn boundary), **not per token** —
   `OutputTextDelta` does not emit it. So it's a handful of small append writes per
   turn absorbed by the page cache, not a hot path.
3. **IMAP client "always fetches full RFC822 and scans the whole message just to
   read flags"** — `email_native.rs:2105`. *Refuted:* `from_utf8_lossy` returns
   `Cow::Borrowed` (zero-copy) for valid UTF-8, and `.lines().find()` short-circuits
   at the FLAGS line (top of the FETCH response). The real over-fetch is the *verify*
   call site — captured separately as M18.
4. **Matrix-cell/rubric scoring N+1** — `src/core/report/scoring.rs:92`. *Refuted:*
   the pattern is literally present, but `scoring.rs` is **dead code** — `report/mod.rs`
   never declares `mod scoring;`, and a deliberate syntax-error probe + `cargo check
   --bin ctox` confirmed rustc never compiles the file. Zero execution paths. (Real
   issue: an orphaned/unwired module island, not a perf hotspot.)
5. **`add_claim`/`upsert_cell` validate evidence_ids one `EXISTS` at a time** —
   `src/core/report/claims.rs:449`. *Refuted:* zero callers anywhere; the `EXISTS`
   hits a `PRIMARY KEY` (O(log n) point probe), and K is a handful. Dead, indexed,
   and tiny.

> The two `mission-report` rejections both point at the same structural smell —
> `report/{scoring,claims,runs,store,state_machine,scope,evidence}.rs` form an
> **orphaned module island** not wired into the `ctox` binary. Worth a separate
> cleanup ticket (dead code, not performance).

---

## E. Recommended remediation order (highest leverage first)

1. **RxDB SQLite predicate/index/`LIMIT` pushdown + read-connection split**
   (H1 + M1, M2, M3 + several lows). Wire `query_planner.rs` into the SQLite
   backend; push `WHERE`/`LIMIT`/`COUNT(*)`; open per-reader read-only connections.
   *Single biggest win for the whole data plane.*
2. **Decouple diagnostics from the hot frame path** (H2 + M24 + `encodedSize` low) —
   cheap debounce/gate, immediately lifts sync throughput.
3. **IMAP/email projection + watermark + index** (H3 + M16, M17, M18, M19). The
   `stalwart_messages` index is a one-liner with linear payoff.
4. **Business OS keystroke path: `Map` index + debounce + reconcile instead of
   `innerHTML`** (H5, H6 + M25, M26, M27, M28 + shell M22) — directly serves the
   "OS-snappy" invariant.
5. **Inference reserve-once** (M11 arena + M12 graph) — per-token host overhead,
   multiplied across every generation.
6. **Async hygiene** (M14 `block_in_place`, M15 bounded mpsc, M2 read-split,
   store `PRAGMA synchronous=NORMAL`) — stability under load + fsync cost on the
   Documents/iCloud filesystem.
7. **Native DB-reopen / N+1 cleanup** (M5, M6, M20, M21 + mission/report lows) —
   many small mechanical wins.

### Quick wins (low-risk, isolated commits)
- `PRAGMA synchronous=NORMAL` in `store.rs:697` and `persistence.rs:197`.
- `CREATE INDEX idx_stalwart_messages_mailbox_received` (M16).
- `CREATE INDEX` on `business_commands(module, command_type, status, observed_at_ms)`.
- TTL-cache the `ps -axo` status scan (`service.rs:3461`, M-`service-loop`).
- Delete the redundant `rows.sort_by` in `report/runs.rs:133`.
- Debounce the WebRTC transport-status emit (H2 / M24).

---

*Generated from workflow run `wf_4972e381-c51`. Findings cite `path:line` at the
time of audit (2026-06-24); verify against current source before fixing, as parts
of the worktree had uncommitted changes (`cv-print-builder`, `store.rs`,
`invoices.rs`, `ats_gates.rs`, …).*
