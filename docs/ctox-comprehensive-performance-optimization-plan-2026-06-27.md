# CTOX Comprehensive Performance Optimization Plan - 2026-06-27

Source review:
`/Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md`

Workspace:
`/Users/michaelwelsch/Documents/ctox.nosync`

## Verdict

The 2026-06-24 performance review is covered by the current plan set, but it is
not fully handled.

No HIGH, MEDIUM, or LOW item from the source review is missing from the plan
set. The remaining risk is not coverage; it is closure. Several findings are
still `partial` or `open`, and there is still no release-valid installed
`ctox-real` idle proof after the real `ctox upgrade --dev` path plus Business OS
and file-access warmup.

The release bar is:

- installed daemon, not just local test binary;
- real `ctox upgrade --dev` path;
- Business OS opened and synced;
- file access granted and file/blob paths exercised;
- warm idle state;
- passive status-free CPU sample below budget;
- no DB/WAL growth and no sync-run / projection / native-peer counter churn;
- status-poll load measured separately.

## Review Method

This plan consolidates:

- the source review from 2026-06-24;
- the existing 2026-06-25 and 2026-06-27 performance plans;
- nine read-only subagent reviews:
  - review coverage matrix;
  - native daemon idle loops and broad stamps;
  - browser Business OS / RxDB / WebRTC / file demand;
  - Rust `ctox-rxdb` SQLite adapter architecture;
  - installed idle gate and release-evidence requirements;
  - current coverage recheck against the source review;
  - current native SQLite/RxDB fallback and lock review;
  - current daemon/file/retention idle review;
  - current browser/WebRTC/UI hot-path review;
- a targeted code-level check of the native demand-file chunk SQL path.

## Coverage Matrix

Status terms:

- `fixed/exact`: reviewed hot path is addressed with source/test evidence.
- `structurally closed`: no known equivalent path remains in the architecture,
  and regression tests cover the contract.
- `release-proven`: structurally closed plus installed `ctox upgrade --dev`
  evidence from the real daemon, Business OS, file-access warmup, and idle
  gates.
- `partial`: reduced, but a structural tail or release-evidence gap remains.
- `open`: reviewed behavior remains.

| Severity | Fixed / exact | Partial | Open | Missing |
| --- | --- | --- | --- | --- |
| HIGH | H2, H3, H4 | H1 | H5, H6 | none |
| MEDIUM | M3, M5, M7, M8, M13, M14, M15, M16, M17, M18, M21, M30, M31 | M1, M2, M4, M6, M9, M10, M19, M20, M24, M29 | M11, M12, M22, M23, M25, M26, M27, M28 | none |
| LOW | L-store-4, L-store-5, L-service-1..5, L-rxdb-native-1..2, L-browser-1, L-browser-3, L-exec-1..2, L-mission-1..5, L-shell-1 | L-store-1, L-store-2, L-store-3, L-browser-2, L-exec-3, L-async-1 | L-infer-1..3, L-exec-4, L-shell-2..4, L-module-1..3 | none |

The source review counts 7 HIGH findings in the severity table while naming
H1-H6. That is explained by H1 being a shared root counted in both `store-sqlite`
and `rxdb-native`; it is not a missing item.

## 2026-06-27 Subagent Verification Findings

Three read-only verification agents rechecked the plan after the 2026-06-24
review. Their conclusion is that coverage is present, but release closure is
not.

Additional blockers to keep explicit:

- H5/H6 and M22-M28 module UI findings need module-specific 1k-record perf
  smokes and budgets; they are not release-proven by daemon idle tests.
- H1/M1/M2/M10 are still only partial until normal `.query()` / `count()`
  fallback paths are either compiled, forbidden for hot paths, or release-gated
  by fallback counters.
- M11/M12 and L-infer-1..3 need a benchmark matrix: per-token host ms, graph
  build count, arena allocation count/bytes, vocab argmax cost, and embedding
  dequant cost.
- M19 mail sync still needs first-import pagination, UIDVALIDITY reset handling,
  and IDLE/provider delta-token proof.
- The orphaned report-module island from the rejected/downgraded review list
  needs an explicit decision: delete, archive, or wire into the active report
  path.
- A `Done` entry is not `release-proven` unless the installed post-file-access
  idle artifacts exist.

Additional gate hardening from the release-gate review:

- Gate A must compare service-performance artifacts from the same process
  identity and fail on missing, mismatched, or reset counters.
- Gate B must prove its intentional status-poll load is actually visible in the
  daemon status-request counters.
- CPU sampling must eventually cover all `ctox-real` candidates, the selected
  process group, and child processes instead of validating only one PID.
- DB growth gates must discover all `runtime/*.sqlite3` and `runtime/*.db`
  files, not only the four currently listed defaults, and must fail per DB file
  component (`main`, WAL, SHM, journal), page count, freelist, dbstat bytes,
  and RxDB row/payload/tombstone deltas.
- Default release counters must be required: a default metric pattern that
  matches no counter should fail the release gate, not only warn.

2026-06-28 installed-gate status:

- Fixed in the gate tooling: `release-identity.json` records `ctox version`,
  accepts the current release-root layout when no shared launcher
  `ctox-real` exists, and ties the sampled process hash to the `current`
  release binary.
- Fixed in the probe tooling: the DB file-growth window starts after read-only
  SQLite pre-sampling diagnostics, so probe-created SQLite `-shm` files are no
  longer misreported as daemon idle growth.
- Current short installed Gate A evidence: release identity passed; passive CPU
  sampling passed (`avg 0.023%`, `p95 0.155%`, `max 0.3%`); DB file growth was
  0 bytes.
- Current blocker: the installed daemon does not yet expose
  `runtime/service-performance.status.json`, so Gate A correctly remains red
  on missing service-performance deltas until that service instrumentation is
  included in the installed release and re-tested through `ctox upgrade --dev`.

Additional subagent findings from the 2026-06-28 architecture/performance pass:

- `desktop_file_index` still falls back to frequent polling when scan roots
  exist but the filesystem watcher is unavailable; it needs watcher-state
  telemetry plus a stable-root slow backoff or partial watcher strategy.
- `business_records` idle projection still pays for a composite stamp; queue
  and chat-repair work should be split into a slower/event-driven loop with an
  explicit projection clock.
- `module_catalog` still rescans/hashes module file trees every 60s; module
  lifecycle writes should maintain a catalog dirty marker or projection clock.
- `channel_state` still opens/hashes Core DB and stats per-channel artifacts on
  idle ticks; this needs a channel projection clock and artifact watcher/cache.
- Native RxDB risks remain: DB-wide read serialization around read-only
  operations, per-row changed-table trigger amplification, unindexed compiled
  JSON SQL scans, per-collection read-connection caches, and external poller
  deduplication across storage instances.

Additional RxDB/SQLite architectural risks:

- Business-record projection cursors must prove they do not skip records when
  more than one sync batch shares the same `updated_at_ms`; cursor state should
  include `(updated_at_ms, record_id)` or explicitly drain short batches.
- Startup/recovery paths such as browser-session recovery must not use
  unsupported Mango operators that trigger normal `.query()` full-scan fallback.
- Desktop chunk cleanup needs an `EXPLAIN QUERY PLAN` guard and large-fixture
  runtime budget for stale-chunk maintenance.
- WAL/SHM/DB plateau must be proven after frame, chunk, Knowledge, and blob
  churn, not inferred from individual cleanup helpers.
- Payload-wide module/user/release stamps should move to change clocks or
  metadata stamps before they become idle-period table scans.
- Knowledge projection tombstone cleanup must drain more than 10k stale rows
  across idle cycles without requiring unrelated source changes.

## 2026-06-27 Current Recheck Addendum

Four additional read-only subagents rechecked coverage, native SQLite/RxDB,
daemon/file/retention paths, and browser/WebRTC/UI paths against the current
checkout. Their result did not change the rollup: the review is covered by the
plan, but it is not fully handled.

New or sharpened findings from this pass:

- Resolved in this pass: startup `browser_sessions` recovery no longer queries
  with `status: { "$ne": "stopped" }`. It now uses a positive
  `status == active` selector, and a regression test proves the recovery path
  does not increment `query_fallback_calls`.
- Normal native `.query()` remains a compatibility fallback risk, but it is no
  longer an unbounded silent full-table scan by default: unsupported Mango
  operators now use `prepared_query.queryPlan` to build SQL candidate bounds
  before the Rust matcher runs, and broad fallbacks without useful candidate
  bounds fail after the fixed scan limit. Per-collection, per-operator, and
  collection/operator fallback attribution is now in the SQLite runtime
  counters and default heartbeat budgets. Ordinary native callers still need a
  hot-query registry.
- File-access idle remains unproven. The installed gate runs upgrade,
  passive-idle, status-poll, and process-mining gates, but it still does not
  automate the representative sequence: open Business OS, grant file access,
  share/open/materialize files, warm idle, then capture a 10-minute passive
  idle window.
- Desktop file indexing still wakes on a short cadence and has a fallback full
  scan path after the unchanged-root window. The improvement is real, but it
  still needs watcher/dirty-root triggering plus counters for roots statted,
  entries statted, fallback scans, files considered, files indexed, no-op
  upserts, maintenance deletes, and direct SQLite statements.
- Demand file fetch is demand-only but not end-to-end streaming. Native fetch
  still loads/sorts chunk rows and decodes base64 into memory before emitting
  slices, and browser demand collectors buffer chunk arrays before resolving.
  Large file access can therefore create CPU/memory bursts that a fresh-daemon
  idle gate would miss.
- Runtime DB growth is still a first-class risk. Current read-only runtime
  evidence observed `business-os-rxdb.sqlite3` around 264 MB, with
  `desktop_file_chunks` around 102.6 MB, `desktop_files` around 58.1 MB, 37,577
  `desktop_files` rows, and 32,840 deleted/tombstone rows. This needs a
  replication-horizon-safe retention contract, WAL/freelist policy, and
  offline reconnect soak.
- Browser/WebRTC reductions are partial: transport-status snapshots are
  throttled and skinny by default, but shared-peer status still fans out to
  collection states; unsupported unbounded IndexedDB queries can still hit
  `allDocuments()`; complex live queries still re-exec; local push scans can
  inspect up to `batchSize * 50` rows; and file-demand transport buffers whole
  chunk result arrays.
- H5/H6 remain open in the active UI: Matching still linearly scans
  requirement/object matches and rebuilds normalized search haystacks on
  undebounced input; Outbound still recomputes `currentPipeline().find()` per
  company row. M23/M25/M27/M28 also remain open for window layout, spreadsheet,
  Buchhaltung, and Customers interactions.

Plan changes from this addendum:

- Keep `browser_sessions` recovery in the hot-query registry so it cannot
  regress to unsupported Mango fallback.
- Add compiler-output `EXPLAIN QUERY PLAN` tests for representative Mango
  queries, not only hand-written SQL fragments.
- Keep normal fallback rows/decoded bytes, indexed-candidate fallback calls,
  too-broad fallback aborts, and collection/operator attribution in the
  post-file-access installed idle gate.
- Add file-index and demand-file counters to the native heartbeat or an
  equivalent status-free perf artifact.
- Add strict browser/runtime counters for `allDocuments()` fallback,
  complex-live-query full re-exec, local push scan rows, pending file-demand
  collectors, and peak retained bytes.
- Add 1k-record UI perf smokes for Matching, Outbound, Spreadsheet, Chat,
  Buchhaltung, Customers, and CV Print Builder.

## 2026-06-27 Subagent Recheck Corrections

The latest read-only subagent pass confirmed full coverage but corrected some
over-optimistic status entries:

- H4 is fixed for the exact tracked-message N+1 reviewed on 2026-06-24:
  browser Chat now batches tracked command/task lookups and keeps the fallback
  timer active only while tracked messages exist. Remaining Chat work is M22
  DOM/layout work, not the original H4 DB round-trip pattern.
- M22 remains open, not partial. Chat still rebuilds message HTML rather than
  reconciling append-only DOM signatures.
- M24 is partial. H2's per-frame heavy native transport-status snapshot is
  fixed, but sync diagnostics still rebuild/sanitize broad objects on coalesced
  publication and need observer/fanout budgets.
- L-browser-2 is partial. Immediate-push bursts are reduced, but the local push
  scan multiplier remains and must be budgeted or replaced with a local-origin
  dirty index.
- L-async-1 is partial. Checkpoint payload reads are cached/reduced, but current
  checkpoint collection still has a sequential-await path and is not fully
  closed by bounded parallelism.

New file-access findings to carry into the work order:

- Passive installed idle must fail if `rxdb_sqlite.external_poll_data_version_reads`
  or `rxdb_sqlite.external_poll_changed_table_reads` advance after file-access
  warmup.
- Post-file-access idle must also gate
  `rxdb_sqlite.changed_documents_since_calls`,
  `rxdb_sqlite.changed_documents_since_results`,
  `rxdb_sqlite.bulk_write_rows`, and `loops.desktop_file_index.rows`.
- Demand-file normal reads must prove canonical chunk lookup succeeded.
  Fallback prefix-range scans are exceptional and should fail the file-access
  scenario unless the test explicitly exercises fallback.
- File Viewer large text previews now pass a bounded `{ offset, length }`
  range to `rxdb.file.fetch`; full downloads remain full reads with hash
  validation. Remaining browser file consumers still need streaming/range
  conversion and peak-retained-byte gates.

## Primary Idle Failure Model

The most plausible sustained-idle CPU failure is a feedback loop:

1. A polling loop wakes even when no owner-visible work is pending.
2. It computes broad file/DB stamps, performs periodic health reads, or writes
   no-op metadata.
3. The DB/WAL/SHM or runtime-store stamp changes.
4. Other gates interpret the broad stamp change as real source activity.
5. Router, projection, ticket, file, audit, or status work reopens.
6. The daemon remains busy although no external source changed.

This is why the fix must be systematic: source-specific stamps, zero-write
no-op paths, hard row/statement/open counters, bounded queues, and installed
idle evidence are required together.

## Release-Blocking Work Order

### P0 - Installed Idle Gate Must Become Release-Authoritative

Problem:
The installed gate runner exists and now proves the sampled PID against the
installed release identity, but it does not yet reproduce the full file-access
scenario and still needs broader release-counter budgets.

Tasks:

1. Record and enforce release identity:
   - git commit;
   - `ctox --version`;
   - installed binary path;
   - `current` symlink target;
   - resolved `ctox-real` PID command path;
   - PID start time.
2. Fail the gate if the sampled process does not belong to the new installed
   release.
3. Add external-status quiet proof:
   - service-side status request counter for IPC and HTTP status paths;
   - process identity in `runtime/service-performance.status.json`;
   - Gate A must show delta 0 for status requests from all sources, not just
     from the probe.
4. Split DB growth budgets per DB and per file:
   - main DB;
   - WAL;
   - SHM;
   - page count;
   - freelist;
   - top RxDB collection payload/tombstone bytes.
5. Extend sync-run gates:
   - row-count deltas;
   - max timestamp / checksum / update-counter changes;
   - defaults for both channel and ticket sync in the standalone probe.
6. Make native-peer heartbeat counters required:
   - schema version;
   - freshness;
   - `replicationUp`;
   - root-stamp ticks;
   - stat calls;
   - fallback scans;
   - maintenance rows;
   - SQLite opens/statements;
   - writer-lock wait/held p95.
7. Add a reproducible file-share scenario runner:
   - open Business OS and wait for sync;
   - grant file access;
   - materialize/read a defined large file;
   - Explorer upload/import;
   - Documents/Spreadsheets blob write/read;
   - CV Print Builder file path;
   - optional browser reconnect;
   - warm idle;
   - Gate A/B/C.
8. Expand CPU scope:
   - enumerate all `ctox-real` processes;
   - require exactly one release-matching process, or aggregate every
     release-matching candidate plus process-group children;
   - fail on extra stale `ctox-real` processes.
9. Make default release counters required:
   - missing heartbeat / service-performance patterns fail;
   - counter resets or negative deltas fail;
   - stale artifact boot identity fails.

Acceptance:

- `ctox_installed_idle_gate.py` can fail a wrong-binary/wrong-PID sample.
- Gate A is status-free globally.
- DB/WAL/SHM and sync-run metadata are flat during idle.
- Missing native counters fail the gate.
- The file-access scenario is repeatable and produces artifacts under
  `runtime/perf/installed-idle-*`.

Status on 2026-06-27:

- Done: release identity records source git commit/branch/status,
  `ctox --version`, install manifest, `current` symlink target, current-release
  and shared-launcher `ctox-real` hashes, sampled process command/path/hash,
  process start time, and upgrade timestamps. A real run fails before Gate A
  when this identity cannot tie the sampled PID to the installed release.
- Done: native RxDB SQLite runtime counters now include statement elapsed
  total/max/buckets and writer-lock wait/held total/max/buckets, and the idle
  probe's default heartbeat budgets fail on those deltas during passive idle.
- Done: external-status quiet proof is implemented with daemon-side status
  request counters for IPC and HTTP status paths,
  `runtime/service-performance.status.json`, process PID/boot identity in that
  artifact, and a default Gate A zero-delta budget for
  `status_requests.total_requests`, `status_requests.ipc_status_requests`, and
  `status_requests.http_status_requests`. Gate A now fails on missing artifacts,
  wrong artifact PID, boot-ID changes, and negative counter deltas. Gate B skips
  the passive file-delta check because it deliberately creates status load, but
  now separately requires the status-poll load to show up as daemon
  `status_requests.total_requests` growth.
- Done: the probe now records all `pgrep -x ctox-real` candidates, aggregates
  candidate CPU, samples the selected process group plus selected-PID
  descendants, and fails passive idle when extra `ctox-real` candidates or
  aggregate scope CPU exceed the configured budgets. The installed gate also
  fails before Gate A when a real run sees an extra `ctox-real` process.
- Done: runtime DB file discovery now includes known CTOX SQLite files plus
  `runtime/*.sqlite3` and `runtime/*.db`, records main/WAL/SHM/journal sizes,
  and the default idle budget fails on positive growth for any component.
- Done: database metric snapshots now run around the CPU window when DB
  diagnostics are enabled. The idle gate can fail on positive page count,
  freelist, dbstat bytes, RxDB collection row/data/tombstone, and sampled
  desktop chunk deltas through default budgets or `--max-db-metric-delta`.
- Done: default heartbeat, service-status, service-performance, and sync-run
  metric patterns now fail the idle assertion when no metric matches instead
  of only warning. This makes older or incomplete runtime artifacts visible to
  the release gate.
- Done: native-peer heartbeat health is now release-gated. Passive idle
  requires `runtime/business-os-rxdb-peer.status.json` to exist before and
  after sampling, to match `ctox-native-rxdb-peer-status-v1`, to belong to the
  sampled PID, to be fresh within the configured age budget, to report
  `running=true` and `replicationUp=true`, and to expose the expected native
  performance, SQLite, and RxSubject counter schemas.
- Still open: replication-horizon-safe retention/compaction policy,
  file-access scenario automation, and real post-file-access installed idle
  artifacts.

### P0 - Remove Broad-Stamp Idle Feedback

Problem:
Several loops still use broad Core/RxDB file stamps or periodic epochs. These
can be invalidated by unrelated DB writes and can reopen expensive idle work.

Tasks:

1. Replace Business OS ticket projection's broad `ticket_store_change_stamp`
   with semantic ticket-table stamps:
   - relevant counts;
   - `MAX(updated_at)` / row-version high-water values;
   - optional hash over projection-relevant columns only;
   - broad DB file stamp only as fallback with a metric.
2. Split `queue_chat_repair` out of the main business-record projection stamp.
   The 10-minute epoch must not reopen the full projection path.
3. Move repair to:
   - real queue/chat source changes;
   - due candidate rows;
   - explicit maintenance windows with counters.
4. Convert `live_service_settings` from whole Core-DB stamp to typed config,
   profile, runtime-env, and secret-store stamps.
5. Cache environment overlays separately. Do not scan `env::vars()` on ordinary
   router/sync ticks.
6. Split runtime settings projection:
   - stable config projection;
   - volatile health projection.
7. Gate volatile health projection on active Business OS sessions or a coarse
   backoff with explicit counters.
8. Instrument router, mission, schedule, projection, and file-index loops:
   - tick count;
   - skipped count;
   - stamp computation ms;
   - DB open count;
   - rows scanned;
   - writes.

Acceptance:

- Sync-run metadata writes do not reopen ticket projections, queue caches,
  router ticks, app recovery, harness audit, or business-record repair.
- A no-op 10-minute window does not perform full business-record projection.
- Idle loop artifacts explain every wakeup from source-specific evidence.

### P0 - File Access And Native File Indexing

Problem:
File access no longer necessarily means chunk collections are always active,
but the native desktop-file indexer still performs periodic metadata/stat work
and bounded fallback scans.

Tasks:

1. Reduce the 15-second root scan to a cheap root-summary check with counters.
2. Make the 5-minute fallback scan conditional on:
   - active file subscriptions;
   - explicit file-demand;
   - changed root summary;
   - or a much slower maintenance interval.
3. Persist source-specific root/child high-water data so stable roots skip
   traversal.
4. Add counters:
   - roots statted;
   - children statted;
   - root-summary/stamp rows or directory entries touched;
   - fallback scans;
   - files considered;
   - files indexed;
   - no-op upserts;
   - deleted/pruned rows;
   - direct SQLite maintenance statements;
   - maintenance elapsed ms and writer-lock held ms.
5. Add an idle gate after file access that fails if file-index counters advance
   continuously after warmup.

Acceptance:

- Granting file access causes at most a bounded warmup burst.
- After warmup, file-index loop work is zero or near-zero with unchanged roots.
- No full recursive or broad fallback scan appears in steady idle artifacts.

### P1 - RxDB SQLite Adapter Correctness And Capability Boundary

Problem:
The Rust SQLite adapter now pushes simple selectors, sort, limit, count, and
get-by-id into SQLite, but it is not a full Mango planner. Unsupported normal
`.query()` paths can still fall back to Rust full-scan/deserialization.
Additionally, SQL-compiled null comparisons are semantically wrong today:
`$eq:null` and `$in:[..., null]` use SQLite `=` / `IN`, which do not match
`NULL`.

Tasks:

1. Fix null semantics before expanding the planner:
   - `$eq:null` -> `expr IS NULL`;
   - `$in` with null -> `(expr IN (...) OR expr IS NULL)`;
   - parity tests against the Rust/Mingo matcher for missing fields and nulls.
2. Document the SQLite selector capability matrix:
   - supported operators;
   - unsupported operators;
   - fallback behavior;
   - hot paths where fallback is forbidden.
3. Make unsupported hot-path query fetches fail fast with structured
   `SQLITE_QUERY_STREAM_UNSUPPORTED`.
4. Keep normal `.query()` fallback counters visible:
   - fallback calls;
   - rows visited;
   - docs decoded;
   - count fallback calls.
5. Centralize EXPLAIN guards for:
   - indexed selector windows;
   - count windows;
   - get-by-ids;
   - cleanup;
   - changed-docs;
   - Business OS hot queries;
   - demand-file chunk prefix ranges.
   These guards must exercise the Mango compiler output, not only equivalent
   hand-written SQL fragments.
6. Add a cursor-correctness gate for Business Record projections:
   - seed more than one projection batch with identical `updated_at_ms`;
   - sync repeatedly;
   - prove no record is skipped and cursor state advances by
     `(updated_at_ms, record_id)` or an equivalent drain contract.
7. Add a native hot-path unsupported-operator ratchet:
   - startup/browser-session recovery;
   - queue/chat repair;
   - projection loops;
   - command/status lookups.
   These paths must keep `query_fallback_calls` and decoded fallback rows at
   zero under release idle gates.
8. Attribute fallback counters by collection, selector operator family, sort
   shape, and caller/hot-path tag where available. Post-file-access idle must
   fail if fallback rows or decoded bytes grow unexpectedly.
9. Add stale desktop chunk cleanup gates:
   - `EXPLAIN QUERY PLAN` with large fixtures;
   - no table scan and no temp sort;
   - writer-lock held time and WAL growth under budget.
10. Add Knowledge projection drain gates:
   - more than 10k stale Knowledge docs;
   - multiple idle cycles drain to zero without unrelated source changes;
   - max payload and DB-growth budgets for projected documents.

Acceptance:

- SQL and Rust/Mingo results match for null and missing-field selectors.
- Release idle gates fail if hot-path fallback counters advance.
- Query-fetch never silently full-scans before streaming chunks.
- Projection high-water cursors cannot skip same-timestamp records.
- Cleanup and Knowledge maintenance are bounded and keep DB/WAL growth flat
  after warmup.

### P1 - Demand File / Chunk Flow

Problem:
The native demand-file chunk SQL was still using JSON expression filters and
sorts for generic chunk collections. This has now been tightened for the native
path, but browser demand transport and chunk lifecycles remain partially open.

Done in this pass:

- Native generic demand chunk reads now use deterministic primary-key prefix
  ranges over chunk IDs and filter the narrow result set in Rust.
- The query uses `deleted = 0` and `ORDER BY id`, allowing SQLite to use the
  `(deleted, id)` / primary-key index path.
- Desktop range fetch now keeps the correct base offset for loaded chunk
  windows.
- Regression tests cover:
  - primary-key range plans with no table scan and no temp sort;
  - desktop range streaming;
  - document/spreadsheet blob chunk prefix streaming with prefix-collision
    filtering.

Remaining tasks:

1. Add client-side stream deadlines for `requestQueryFetchOnce()` and
   `requestFileFetch()`.
2. Bound the global demand query queue and in-flight accepts.
3. Ensure collectors are cleaned up if `complete` / `error` never arrives.
4. Add smokes for missing completion/error frames.
5. Make file-demand in-flight dedup range-aware.
6. Add browser `fetchFileStream()` / `ReadableStream` consumers so imports and
   integrity checks do not concatenate full files into retained arrays/blobs.
7. Change browser demand transport so chunk batches are delivered to a
   callback/async iterator as they arrive instead of collecting the full file
   result before resolving.
8. Change native demand fetch/write paths so large files are decoded, hashed,
   sliced, and emitted/written incrementally instead of loading full chunk sets
   or whole files into memory where possible.
9. Add peak-retained-bytes guards for File Viewer, Explorer, CV Print Builder,
   Documents, Spreadsheets, and Universal Importer.

Acceptance:

- No unbounded demand queue.
- No permanent collector after lost completion.
- Large files stream through bounded memory on both native and browser sides.
- File-demand probes report pending collectors, chunks buffered, decoded bytes,
  and peak retained bytes.
- Native demand chunk reads cannot regress to full scans or temp sorts.

### P1 - Browser File Metadata And Blob Indexing

Problem:
Explorer and Universal Importer still load broad `desktop_files` lists and
filter in the UI. Documents use `document_blob_chunks.find({ blob_id }).sort(idx)`
without a blob/index schema index on the browser side.

Tasks:

1. Add `desktop_files` indexes for `deleted`, `parent_id`, source/kind/path
   shapes used by Explorer.
2. Replace Explorer broad lists with paged, indexed parent queries.
3. Replace Universal Importer `desktop_files.find().exec()` with paged indexed
   queries or demand APIs.
4. Add row virtualization where lists can exceed the viewport.
5. Add `document_blob_chunks` and `spreadsheet_blob_chunks` indexes over
   `[blob_id, idx]` where browser code reads by blob and sorts by chunk index.
6. Rebuild schema contracts and generated browser/native hash fixtures together.
7. Use indexed cursor / primary-key range cleanup for superseded blob chunks.
8. Stream blob rebuild instead of concatenating one full base64 string where
   possible.

Acceptance:

- Explorer/Importer do not call unbounded `allDocuments()` / full `find()`.
- Blob chunk reads are index-backed in the browser store.
- Schema hash and contract drift tests remain green.

### P1 - Browser RxDB Query And Push Discipline

Problem:
Browser IndexedDB query planning is much better, but unsupported unbounded
queries can still fall back to broad `allDocuments()` reads, complex live
queries can still re-execute on each change, and local-write push scans can
inspect many non-local replicated rows.

Tasks:

1. Add runtime counters for:
   - `allDocuments()` fallback calls;
   - rows read by fallback;
   - complex live-query full re-exec calls;
   - local push scan rows;
   - local push skipped replicated-origin rows.
2. Make interactive and demand-loaded paths strict:
   - reject unbounded unsupported queries;
   - require finite page/window or an indexed cursor;
   - fail tests when query-plan says index but execution falls back.
3. Add delta-aware complex query subscriptions:
   - apply changed IDs when the selector/sort permits it;
   - otherwise invalidate with an explicit counter before full re-exec.
4. Replace local push scan skipping with a local-origin dirty index or durable
   queue so push reads actual local candidates instead of scanning up to
   `batchSize * 50` rows.
5. Add representative browser perf smokes for Advanced Status, Explorer,
   Documents, Spreadsheets, Chat, Matching, and Outbound.

Acceptance:

- Normal Business OS use does not increment `allDocuments()` fallback counters.
- Complex live-query re-exec is rare, attributed, and budgeted.
- Local pushes are proportional to local changes, not collection size.
- Browser fallback counters are included in installed post-file-access evidence.

### P1 - WebRTC Backpressure And Diagnostics

Problem:
Status frames are reduced, but demand/file in-flight gates and WebRTC send
queues are not hard bounded everywhere. Native inline sends can bypass the same
buffered-amount wait used by framed sends.

Tasks:

1. Make send queues hard bounded with drop/backpressure policy.
2. Apply the same buffered-amount wait to native inline sends.
3. Make query/file in-flight limit checks atomic enough that concurrent accepts
   cannot exceed the cap.
4. Add fanout/observer counters:
   - status frames produced;
   - status frames sent;
   - status frames dropped/coalesced;
   - observers notified;
   - sanitize time.
5. Add slow-peer soak tests and release-gate budgets.

Acceptance:

- No unbounded send queue.
- Slow peers cannot force sustained CPU or memory growth.
- Diagnostics remain opt-in/skinny during idle.

### P1 - Module UI Hot Paths

Problem:
Several user-facing modules remain open from the 2026-06-24 review. These are
not the primary daemon-idle CPU root when the browser is closed, but they can
produce active-browser CPU and demand/query pressure.

Tasks:

1. Matching:
   - precompute maps for requirements, matches, and objects;
   - debounce search/input;
   - cache search haystacks;
   - avoid per-card `find()` loops.
2. Outbound:
   - memoize `currentPipeline()`;
   - build company-to-pipeline maps;
   - target reloads by changed IDs.
3. Spreadsheets:
   - keep a persistent HyperFormula engine;
   - update changed cells only;
   - avoid full DOM/cell walks per edit.
4. Chat/window shell:
   - avoid full message `innerHTML` rebuild/compare on no-op sync;
   - batch layout reads/writes in drag/resize paths.
5. Buchhaltung and Customers:
   - pre-aggregate entry/line and customer summary maps;
   - render only affected panes/rows;
   - debounce search and avoid full left+center renders on each input.
6. CV Print Builder:
   - remove per-chunk `findOne()` existence checks before bulk writes;
   - use keyed/batched chunk existence checks or idempotent bulk writes.
7. Add 1k-record perf smokes for Matching, Outbound, Spreadsheets, Chat,
   Buchhaltung, Customers, and CV Print Builder.
8. Attach budgets per interaction:
   - Matching search keystroke;
   - Outbound campaign/filter render;
   - Spreadsheet cell edit;
   - Chat no-op sync and window drag/resize;
   - Buchhaltung journal render/search;
   - Customers search;
   - CV Print Builder file/chunk dispatch.

Acceptance:

- Module perf smokes have stable budgets.
- High-frequency input does not trigger O(collection-size) recompute.
- Browser active use does not create daemon demand/query fallback pressure.

### P2 - Inference And Provider Tail Work

Problem:
The inference and provider findings remain open or partial, but they are not
the leading explanation for file-access idle CPU.

Tasks:

1. Reuse Qwen descriptor arenas / graph contexts.
2. Avoid full-vocab host argmax per slot where possible.
3. Avoid CPU token embedding dequant per decode step.
4. Remove provider transcript clone/parse/reserialize hot paths.
5. Keep API cost/runtime-env batching evidence current.
6. Add benchmark counters:
   - graph build count;
   - arena allocation count and bytes;
   - host argmax ms;
   - embedding dequant ms;
   - per-token host overhead.

Acceptance:

- Local inference decode CPU overhead is benchmarked before/after.
- Provider request building avoids unnecessary full transcript copies.

### P2 - Mail Sync And IMAP Tail Work

Problem:
The IMAP server FETCH/STORE and channel mail-sync findings are reduced in some
paths, but the release plan still needs proof for large mailboxes and initial
syncs.

Tasks:

1. Add first-import pagination gates for large inboxes.
2. Add UIDVALIDITY reset tests so cache/watermark recovery stays bounded and
   correct.
3. Add IDLE or provider delta-token proof where the adapter supports it.
4. Ensure FETCH/STORE projection paths do not load full bodies unless requested.
5. Budget bytes read from SQLite and rows materialized per IMAP/status command.

Acceptance:

- Large mailbox status/flag operations do not fetch full message bodies.
- First import is paged and bounded.
- Steady idle mail sync is event/delta-token driven or backs off with zero
  repeated full UID-list scans.

## Implementation Sequence

1. Harden the installed idle gate first. Without this, no code fix can be
   declared release-clean.
2. Remove broad-stamp idle feedback in native daemon loops.
3. Close the file-access idle loop: native file index, demand stream, browser
   file metadata, and chunk retention.
4. Fix RxDB SQLite null semantics and make fallback counters release-gated.
5. Bound WebRTC queues and demand collectors.
6. Add browser module perf smokes and fix H5/H6/M25/M26 tails.
7. Address mail-sync/IMAP large-mailbox tails.
8. Address inference/provider tails.

## Required Verification

Local code checks:

- `cargo fmt --check` or targeted `rustfmt --check` with the repo edition.
- Targeted Rust tests for changed daemon/RxDB paths.
- `cargo test --manifest-path src/core/rxdb/Cargo.toml` after adapter changes.
- `node src/apps/business-os/rxdb/tests/run-all.mjs` after browser RxDB changes.
- Schema/hash/contract drift guards after collection schema changes.
- `git diff --check`.

Release checks:

- `ctox upgrade --dev`.
- Installed `ctox-real` PID identity proof.
- Gate A passive idle with no status requests.
- Gate B status-poll load measured separately.
- Gate C process-mining/liveness evidence.
- File-access scenario before idle sampling.
- DB/WAL/SHM/page/freelist/tombstone budgets.
- Native peer heartbeat counter presence and budget assertions.

## Current Known Evidence From This Pass

Passed:

- `cmp -s /Users/michaelwelsch/Documents/ctox/docs/ctox-performance-review-2026-06-24.md docs/ctox-performance-review-2026-06-24.md`
- `rustfmt --edition 2024 src/core/business_os/rxdb_peer.rs`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox hot_business_os_schema_indexes_have_sqlite_query_plan_guards -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox demand_file_source_streams_decoded_chunks_in_idx_order -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-service-perf-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox demand_file_source_streams_blob_chunks_by_primary_key_prefix -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-browser-session-recovery-target CTOX_VOXTRAL_BUILD_GGML=0 cargo test --bin ctox browser_session_recovery_uses_indexed_active_query_without_fallback -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-fallback-target cargo test --manifest-path src/core/rxdb/Cargo.toml query_fallback_ -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-fallback-target cargo test --manifest-path src/core/rxdb/Cargo.toml fallback -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/ctox-rxdb-fallback-target cargo test --manifest-path src/core/rxdb/Cargo.toml storage::sqlite::instance -- --nocapture`
- `python3 -m py_compile src/tools/perf/ctox_perf_probe.py src/tools/perf/ctox_installed_idle_gate.py`
- Synthetic `ctox_perf_probe.py` heartbeat-health smoke covering valid,
  stale, `replicationUp=false`, and missing-performance-counter snapshots.

Still missing:

- real installed `ctox upgrade --dev` idle artifact;
- post-file-access installed idle proof;
- full RxDB Rust crate suite after adapter changes;
- browser RxDB suite after any future schema/index changes;
- installed artifact proving release-gated native peer counter presence under
  the real `ctox upgrade --dev` path.
