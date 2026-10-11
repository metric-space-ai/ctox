# CTOX Sync — root-cause classes derived from the fix history (01.09.–10.10.2026)

Read-only analysis of `~/.local/state/workjet-launchpads/ctox-skillboot` (branch `thesen-1009`,
HEAD `089e9270f`). Inputs: `commits.txt` (781 non-merge commits), 9 field findings
(`docs/ctox-sync-feldbefund-*.md`), `docs/ctox-sync-offensive-defects.md`,
`docs/ctox-sync-command-bus-hardening-plan.md`, `docs/ctox-rxdb.md` lines 1–990, and RFC draft
`ctox-skillboot-b/docs/rfcs/ctox-sync-v3.md` §2 (U1–U5).
Full per-commit classification: `classification.csv` (all 781 rows; classes starting with `X` are
excluded, non-fix commits).

## 0. Method and corrections to the input set

**Method.** Every commit got exactly one primary class, based on its subject, file list, body and,
for empty bodies, the doc delta or diff. I read all 175 non-empty commit bodies (first ~350 chars),
read full bodies or diffs for about 25 more, and inspected the doc deltas of 16 fix commits with
empty bodies. Subject-only classification was the fallback for the remaining Codex commits with
empty bodies. Fix-like means fix/repair/prevent/keep/preserve/bound/retry/recover/guard/fence/
release/retire/reject/align/invalidate, plus `perf` commits that fix a measured field defect.
Excluded: features (`XF`), test-only (`XT`), docs/style/format/deps (`XD`), pure stamp bumps
(`XS`), and fixes to domain or UI logic that only touched sync files (`XDOM`).

**The 781 commits are not 781 sync changes.** This corrects RFC §1.1:

| Bucket | Commits |
|---|---:|
| Features (Workjet Supervisor, JourFix, KPI, provider federation, handoff, guest frames …) | 206 |
| Test-only | 118 |
| Domain/UI fixes that touch sync files only incidentally (`XDOM`) | 52 |
| Docs/format/deps (`XD`) + pure stamp bumps (`XS`) | 22 + 17 |
| **Fix-like sync/data-plane commits (classified below)** | **366** |

- 188 of the 781 commits do not touch any path the RFC names (`rxdb/src`, `src/core/rxdb`,
  `rxdb_peer*.rs`, `shared/sync.js`). Another 33 change those paths only by a `?v=` cache stamp.
  Only 560 change them for real.
- The real "sync core" is larger than the RFC paths. 96 of the 366 fixes touch none of those paths;
  they live in `src/core/business_os/store.rs` (48,808 lines), `command_plane.rs`,
  `mission/channels/command_saga.rs` and `mcp_channel.rs`.
- The set also misses app-side compensation. Since 01.09., 42 commits in `modules/` and
  `customer-modules/` touched readiness, profile or query-authority semantics; 35 of them are not
  in `commits.txt`. They include the most damaging regression of the period, `dc0a0591b` (leads
  made demand-only), and its re-fix `1773924d1`.
- Stamp noise: 115–127 of the 366 fixes touch `db.js`, `index.html`, `rxdb-runtime.js` or
  `sync.js` only to bump `?v=` stamps.

## 1. Class table (366 fix-like commits)

| Class | Meaning | n | % | Sep | Oct 1–10 |
|---|---|---:|---:|---:|---:|
| **U2** | Several stores, projection chains, no write ownership | 59 | 16.1 | 32 | 27 |
| **AUTH** *(new)* | Authority/capability/permission boundary checked by hand in each path | 43 | 11.7 | 22 | 21 |
| **ASSET** *(new)* | Hand-kept cache-buster / shell-generation stamps, asset delivery | 36 | 9.8 | 25 | 11 |
| **U1** | Implicit collection profile and readiness contract | 31 | 8.5 | 20 | 11 |
| **GEN** *(new)* | Generated contracts and registries drift (JS vs Rust, command inventory counts) | 29 | 7.9 | 10 | 19 |
| **LIFE** *(new)* | Resource and lifecycle ownership (slots, leases, observers, callbacks, restart) | 28 | 7.7 | 16 | 12 |
| **U4** | Transport: session/responder lifecycle, admission and flow control, head-of-line | 24 | 6.6 | 18 | 6 |
| **U5** | Parallel changers: integration repair (compile, merge, lint, cherry-pick) | 22 | 6.0 | 5 | 17 |
| **CMD** *(new)* | Command-bus semantics: admission vs completion, idempotency, effect claims | 18 | 4.9 | 8 | 10 |
| **OBS** *(new)* | Diagnostics added after the fact (failure cause or stage not visible) | 17 | 4.6 | 6 | 11 |
| **U3** | Oversized or hot+cold documents, unbounded growth, count-based batches | 13 | 3.6 | 6 | 7 |
| **QRY** *(new; data side of U1)* | Demand-window, invalidation and live-query semantics (refetch storms) | 12 | 3.3 | 2 | 10 |
| **SQL** *(new; sharpens U2)* | SQLite access model: connection per request or collection, deferred-txn upgrade, locks | 9 | 2.5 | 9 | 0 |
| **REPL** *(new)* | Replication protocol: checkpoint validity, HLC anchoring, false ACKs, equal-clock conflicts | 8 | 2.2 | 7 | 1 |
| **SCHEMA** *(new)* | Schema/version/migration drift (DB6, optional registration, legacy rows) | 6 | 1.6 | 4 | 2 |
| **IDB** *(new)* | IndexedDB journal and cache lifecycle and bounds | 6 | 1.6 | 2 | 4 |
| **MTAB** *(new)* | Multi-tab leader/follower | 4 | 1.1 | 4 | 0 |
| **UNCLEAR** | Not classifiable with confidence (`5e81f8e65`, Office staging repair) | 1 | 0.3 | 1 | 0 |

**Trend.** September had 197 fix-like commits (6.6 per day). October 1–10 had 169 (16.9 per day,
×2.6). By ISO week: KW36 23, KW37 80, KW38 11, KW39 48, KW40 62, **KW41 142** (05.–10.10.).
In KW41, U2 (20), GEN (19), AUTH (19), U5 (15) and OBS (11) dominate. The fix rate is
accelerating, not converging. Authors of fix-like commits: Codex 262 (72%), Michael Welsch 101,
other 3. Seven commits landed marked `[UNVERIFIED]`.

## 2. Classes: root cause, top sub-patterns, RFC fit

### U2 — Several stores, projection chains, no write ownership (59)

**Root cause.** The design keeps a fact in up to five places: core SQLite,
`business-os.sqlite3` `business_records`, `business_command_aggregates`, the RxDB table and the
browser replica. Several writers can write each copy: projection pumps, native handlers, browser
push and MCP writes. Ownership is not declared per collection or field. Every new consumer or
writer therefore has to be fenced locally against the others.

Top sub-patterns:
- **Projection writers hold the SQLite write lock (9).** `79ee5a2a4`, `5f7ba5dcb`, `38a03a913`
  (the COUNT(*) trigger kept the store write-locked 87%), `2a2a75bf1`, `ca7f7f8e5` (a restart
  replay took over 25 min), `994817ad6`, `f2d0b7b9c`, `f4b86ec5b`, `6876aa22a`. The chain runs
  from 08.09. to 10.10. and is still active.
- **Projection writes invalid RxDB envelopes, repaired in place (7, all on 23.09.).**
  `5306cffb3`, `f3eaa25b3`, `948b8bcb1`, `eb7307b50`, `97569c137`, `78e41ab43`, `e4fefbf90`, plus
  the expectation fix `138ec4fed`.
- **Projection mirror drops or alters canonical fields (6).** `af217dd41`, `b3cc0719a`,
  `b3268ae38`, `0106a9712`, `bad522cbb`, `4e88d7014`.
- **Shadow table trusted over native (5).** `6259bfac9` (MCP reads shadow before native),
  `20d2f9329`, `05cee78a9`, `1ea420bcd`, `57f0887ce` (a forged public shadow).
- **Two sources for one state (5).** Queue vs command lifecycle: `5471f700b`, `1dbd63a69`,
  `e7b0458b9`, `bba741173`; app copy vs registry: `ff6e36212`.
- **Concurrent writers of the same field (3).** `21026deca`, `51deb657b`, `d6cd2b1f3`; related:
  `2536dd8b6`, where the browser re-pushed native-owned fields forever (157 writes stuck), and
  `64ac2f83b`, where stale browser copies set 151 leads to failed.
- **One datum copied into every store (2).** Secrets reached five tables: `e23e12c87`,
  `718ce57f8`.
- **Projection clock and cursor ordering (3, all 10.10.).** `e81ff717f`, `e99d9e92d`,
  `6be8be119`.

**RFC fit.** U2 is correct and is the largest class. Sharpen it: the fault is not "three files"
but no declared per-field ownership, plus shadow and compat tables still read as fallbacks, plus
projections writing directly into RxDB storage tables, bypassing the RxDB contract.

### AUTH — Authority checked by hand at every await point (43; not in the RFC)

**Root cause.** Authority (role, grant, capability epoch, issuer fence, owner alias) is
re-evaluated imperatively inside each handler and each async step. It is not an immutable,
connection-generation-scoped context checked by the framework. So every new RPC, window, cache or
publication path needs its own before/after-await fence, and the process-wide fences contend.

Top sub-patterns:
- **#211 control-plane window permission race (8 fixes in 5 days).** `620344d01` (an SWR change)
  → `9b822fcaf` → `393ae16d2` → `ba6fff756` → `c20c4c74c` → `71bce5518`, `c8df361fb`,
  `5e0180305`, `b1eb3d2b8` → `c891c532b`. Each review found another await point.
- **Process-wide issuer fence contention (3, all 07.10.).** `495342bad` (~50 parallel fetches,
  all but one failed), `f693d0c92` (wait 5 s), `de65a5a7b`; followed by misclassified lookup
  failures `215bba754`, `45b9592eb`, `bc1804f38`.
- **Authority must hold until the physical send (4).** `ef5fcd76d`, `e1c8d145d`, `fe9ef70ef`,
  `43ef0572f`.
- **Identity and owner aliases (4).** `e299d62c7`, `7f700b8c4`, `45530f174`, `9d965be2d`.
- **Stale capability and credential caches (3).** `84929abcb`, `86567b015`, `1f66efef8`.

**Sensitivity.** About 22 more commits classed as features are authority hardening of new paths,
for example `eac8d50f5`, `369a20f1a`, `b8616cae9`, `6f10fe172`. Counted in, AUTH would reach about
60. About 11 of the ~36 incident sections in `ctox-rxdb.md` §0–990 are authority sections.

**RFC fit.** Missing.

### ASSET — Hand-maintained shell generation stamps (36 + 17 pure bumps; not in the RFC)

**Root cause.** The browser module graph is versioned by hand-edited `?v=`, `APP_BUILD` and
generation strings in 6–8 files (38 references per release in `912532b4a`). One forgotten
reference gives a mixed or stale graph, and a cached loader can pin an old bundle.

- **Shell generation stamp mismatch (27, on 16 different days).** Examples: `7c197f102`,
  `91f60a718`, `b0b27811a`, `9b2222dc3`.
- **Boot failures (2).** `d714d887a` (`sync.js` imported with a buster from 03.09., so the shell
  did not boot on a tenant) and `15f33c4b9` (a four-hour-cached v341 loader served an old bundle).
- **Multi-tab epoch coupled to the cache revision.** `42d7c3b77`.

These stamps are also the most frequent merge-conflict surface (`9f3d474d3`, `241306380`). Field
defects D01 and D02 in the offensive register belong here.

**RFC fit.** Missing.

### U1 + QRY + MTAB — Implicit collection profile, readiness and query semantics (31 + 12 + 4 = 47, plus 35 app-side commits)

**Root cause.** A collection's profile (eager, demand-only, projected, control) implicitly decides
six behaviors: read path, readiness, push/invalidation delivery, sidecar eviction, checkpoint
validity and follower-tab serving. Whatever value a profile does not define becomes "unknown",
"timeout" or "empty" in each consumer. Each app and shell feature therefore defines readiness,
freshness and known-vs-unknown locally.

Top sub-patterns:
- **Eager vs demand chain (8+).** `ee2259f7d` (the 6 MiB sidecar evicted eager rows; checkpoint
  invalidated; ~20 MB re-pull) → `bea5477e0` (#243, eager answers locally) → `e2f9f5b48` (#243
  dropped the 200-row cap; 871 chats re-persisted; page froze) → `3767c8871` → `ce524fc3a` →
  `9668bb407` (demand-only dropped pushed changes; lead list re-paged every second) → `1004a0ea9`
  (re-announce storm; 40% of a CPU core) → `b1e4bf097`. Then `dc0a0591b` (09.10.) made leads
  demand-only, and `1773924d1` (10.10.) had to redefine "ready" for demand-only: 145 s down to
  42 s.
- **Desktop pin/layout: unknown vs empty (7 in 4 days).** `72b83fecd`, `ea566a3f0`, `d7076b2f7`,
  `cd5899630`, `c74b88f4e`, `8511684e0`, `71395cf18`; then `ae8515b0e`, `5de59b4fa`, `c4cd2d554`.
  The shell wrote defaults before the server state was known.
- **UI blocked on sync warmup or readiness (8).** `e1f5d4c5a`, `a6fd70c06`, `4383933aa`,
  `7b45915a4`, `ae8515b0e`, `817a6d783`, `5de59b4fa`, `d8245a683`.
- **Unknown read as absent or failed (3), stale shown as fresh (2), legacy HTTP readiness faked
  ready (D14).** `04eaa9e73`, `39da2ca53`, `e3b5a32c0`, `a8883576c`, `0ace1ae57`, `d2e719205`.
- **QRY.** Refetch and invalidation storms: `d5e55c089`, `1004a0ea9`, `6c4e7d3a3`, `aa4f9d6c5`.
  The Threads storm of 06.09. caused 15–20 min command latency.
- **MTAB.** Leader handover (`55025687c`, `4fc3ed8da`), follower reads and commands (`f6c119cee`,
  `02aca8804`). In the field finding of 27.09., a follower tab got empty windows.

**RFC fit.** U1 is correct and is the most user-visible class. Sharpen it in three ways: the
contract must cover all six profile-dependent behaviors, not only readiness; every value needs a
tri-state of known, unknown and stale; and maintenance readiness must not wait on ALL collections
(field finding of 09.09.).

### GEN — Contracts and registries maintained by hand on both sides (29; not in the RFC)

**Root cause.** The command inventory length, collection allowlists, module inventories and
some wire schemas are hand-kept counts or lists next to the code that defines them. Rust and JS
also interpret JSON Schema and Mango differently. Every new handler or field therefore needs an
extra "align count/regenerate" commit, which also conflicts across parallel branches.

- **Command inventory count drift (12).** `502c759a4`, `9e6322bf8` ("after main rebase"),
  `1b2a1221d`, `33344807d`, `db7f48f80`, `8451c0b61`, `7cbc4618a`, `c2cbf390d`.
- **Schema type unions JS vs Rust (3 on 06.09.).** `01fe390c7`, `3fffd9f0a`, `151501174`.
- **Mango selector normalization (2, the same patch twice).** `1b3c991de`, `3c5c1cd06`.
- **KPI integers.** `0c7857624`.
- **Dist bundle not regenerated.** `c51e4bf5c`, `f0c1d271c`.

**RFC fit.** Missing as a cause. RFC §3.1 assumes generated contracts as part of the target state.

### LIFE — Lifecycle and resource ownership (28; not in the RFC)

**Root cause.** Slots, leases, bridges, observers, timers and send tasks are owned by whichever
code acquired them, not by a connection-generation or view scope. Cancel, replace, close, role
retirement and restart each leak or double-release something.

- **Stream slot and lease leaks (4).** `ee477f343`, `cadeea2cf`, `cd1962502`, `d9a68952a`.
- **Native query cache keeps closed collections alive (3, on 01.10.).** `5ae3f4690`,
  `c3695eccd`, `722e65844`.
- **Retirement races (3).** `926a3a2cb`, `e83b776d6`, `1a05257c3`.
- **Stale callbacks and timers from a retired connection (2).** `a158f62c6`, `a2aba15fc`.
- **Bridge/lease ownership across runtime replacement (2).** `87abdd514`, `4a842025f`.

**RFC fit.** Missing. It overlaps U4 in the RFC's sense.

### U4 — Transport (24)

**Root cause.** The WebRTC session has no specified state machine for offer renewal, responder
replacement, signaling identity change or transport renewal. Admission and flow control are
inconsistent between the peers. Head-of-line blocking is real but is the smaller part.

- **Responder/offer renegotiation races (7).** `2f8c2966f`, `626e9b29e`, `cc648dbe9`,
  `e0b4c4481`, `3d1c93284`, `82dca7db2` (all 28.09.) and `d66f7eb7e` (08.09.).
- **Head-of-line and priority (4).** `70e031508`, `f8e3ff462`, `51ecc7bf9`, `70a1e6da3`.
- **Inconsistent stream-limit admission (2).** `86098f681`, then `0fc6e7176`: the server rejected
  twice, and the client did not retry the first rejection.
- **Throughput.** `23532e91f` (stop-and-wait ACK window gave ~1 MB/s over TURN), `bd699317c`.
- **Background-tab timers stall ACKs.** `bd52960b9`.

**RFC fit.** U4 is partly correct. HOL and round trips explain only about 6 of 24 fixes. Add
"connection/session lifecycle not specified" and "flow-control protocol inconsistent"; the latter
also produces the LIFE slot leaks.

### U5 — Parallel changers (22)

**Root cause.** Several workers land on `main` concurrently, with no composed build gate and no
single integrator. Global hand-edited singletons (stamps, inventory counts) are guaranteed
conflict points.

- **Main broken by a parallel change (10).** `bcf6ed6b0`, `51f54ace6` and `c06a176ec` (the same
  fix three times), `bbdf01609` + `d76d5db84`, `687dfccd2`, `391442014`.
- **Lint breakage after composition (3).** `473966d9c`, `8fbc576ec`, `323b4d709`.
- **Merge lost behavior.** `266727c23`.
- **Revert.** `9d3bbe83a`.
- **CI red on main.** `3b8ff841a`, eight breakages.
- 11 subjects appear twice or more in the 781 (cherry-picked duplicates).

**RFC fit.** Correct. Sharpen with these numbers: KW41 alone had 142 fixes, and ASSET+GEN are
the conflict magnets.

### CMD — Command-bus semantics (18)

**Root cause.** Admission, execution and terminal state are not one atomic, typed lifecycle. The
`task_id` field has two meanings, side effects can run before the durable claim, and canonical
plus compat writes coexist (hardening plan P0/P1).

- **Effect applied but claim lost (3, on 09.09.).** `e9c01e2c6`, `e24715dde`, `e80c66e2e`.
- **Command class misrouted (3).** `55cdde7a8`, `8a8ea725e`, `e011490ae`.
- **Replay and idempotency.** `c66f2f930`, `e3780654f`.
- **Dual canonical/compat writes.** `a546dce29`.

**RFC fit.** Missing in RFC §2. It is fully described in the hardening plan; it should be
referenced there or folded into U2.

### OBS — Diagnostics retrofitted (17; not in the RFC as a cause)

`72836727a` ("opened then dropped" vs "never opened" cost an afternoon), `ae524d6ad`, `433dd2dca`,
`dd276c309`, `5a28a805f`, `4f28b66e8`, `d7eea14a2`. The field findings themselves contain
measurement errors: the REAL<TEXT comparison of 06.09., `sentFrames` not trustworthy, and
`receivedBytes` reporting 20 MB where 1.2 MB actually crossed the wire (RFC §1.2). U5's
"measurement gates" need validated instruments first.

### U3 — Data size and growth (13)

- **Oversized projected documents stall a whole collection (4).** `e87de4f50`, `287205cc9`,
  `fcce19763`, `ad93f4d21`.
- **Count-based batches.** `f7ba3237f` (100 docs × 73 KiB per `masterWrite`), `3b068e76e`.
- **Eager collections too large, moved to demand.** `8235c3a65` (18.6 MB of chats), `a23ef7765`.
- **Command history growth.** `ceaeb37f0`, `4f24a3ee3`.

**RFC fit.** U3 is correct but over-weighted relative to its fix count (3.6%). Its real cost is
second-order: the clamp introduced by the U3 fix led to `88b4af3f8` (projection maintenance
reduced file-chunk storage to omission markers) and `17b12b15f` (the clamp aborted 12 peer
bring-ups). Budgets must be byte-based.

### SQL, REPL, SCHEMA, IDB (9 / 8 / 6 / 6)

- **SQL.** Deferred-transaction upgrade returns `SQLITE_BUSY` (`0ae16b448`: 62% of lookups failed
  intake; `17b12b15f`). Reader per collection (`e5973c8c3`). Connection reuse and identity fencing
  (`40ef09a60`, `7bb941774`, `3b820f44e`, `9445d257e`). The GC loop held the write lock
  (`0abc3de29`). This sharpens the "database is locked" part of U2.
- **REPL.** The checkpoint was discarded after every server write (`ac482633f`, `383ca22bb`:
  ~53 MB per reload). HLC anchor (`6b9867339`, `fb4ee473d`, `eee71c552`). False ACKs
  (`53efbe0a5`, `7e52ccbd3`). Equal-clock conflicts (`0785246e1`). Matching field findings: the
  write lost after a rapid follow-up write (11.09.), and D06, D07, D15.
- **SCHEMA.** Schema changed without a version bump (`bc96a4b11`, caused by `e0a7cb4dc`).
  DB6 optional registration (`8de040615`). Migrations (`e7c1affc4`, `320bcd6dd`, `cbabdb48d`).
- **IDB.** Recovery-journal scan and payload bounds (`3d34fb470`, `ad96c8a40`, `23a1a93e7`).
  Open/handle lifecycle (`160475679`, `576ea58a0`).

## 3. Re-fixed areas: same area ≥3 times, or a fix that caused the next fix

These are the strongest architecture signals.

| # | Chain | Commits | What it shows |
|---|---|---|---|
| 1 | Projection writer locks | 13 over 33 days (`79ee5a2a4` … `6be8be119`, still active 10.10.) | Several projection writers on one SQLite file (U2/SQL) |
| 2 | Eager vs demand profile | ≥10 (`ee2259f7d` → `bea5477e0` → `e2f9f5b48` → … → `dc0a0591b` → `1773924d1`) | Each fix redefines profile semantics; #243 itself caused `e2f9f5b48` (U1) |
| 3 | #211 control-plane window permissions | 10 in 5 days | Authority re-checked at every await point (AUTH) |
| 4 | Oversized documents → clamp → collateral | `e87de4f50`, `287205cc9` → `88b4af3f8` (file storage destroyed) → `fcce19763`, `ad93f4d21` → `17b12b15f` (12 failed bring-ups) → `3b068e76e`, `f7ba3237f`, `2536dd8b6` | A U3 fix done inside the U2 projection layer generated U2/SQL failures |
| 5 | RxDB envelope repair | 8 on 23.09. | Projection writers bypass the RxDB document contract |
| 6 | Desktop pin/layout | 10 (10.–28.09., 3 dates) | Unknown vs empty not modeled (U1) |
| 7 | Shell generation stamps | 27 + 17 bumps, on 16 days | Hand-maintained global version (ASSET) |
| 8 | Command inventory count | 12 on 6 dates | Hand-maintained global count (GEN) |
| 9 | WebRTC responder renegotiation | 7 (6 on 28.09.) | Unspecified session state machine (U4) |
| 10 | Query stream slots/admission | `51ecc7bf9`, `ee477f343`, `cd1962502`, `86098f681`, `0fc6e7176`, `3b7f64e5e`, `70a1e6da3` | Flow-control contract inconsistent between peers (U4/LIFE). The field finding of 07.10. measured no improvement from `86098f681` |
| 11 | Issuer-fence authority | 6 on 07.10. | Process-wide lock on the authority path (AUTH) |
| 12 | Checkpoint validity | `ee2259f7d` → `ac482633f` → `383ca22bb` | Checkpoint key semantics (REPL) |
| 13 | Schema unions | 3 on 06.09. | Two schema interpreters (GEN) |

The only explicit revert is `9d3bbe83a`. Re-fixes are the norm instead: in 9 of 13 chains, a later
commit corrects or extends the previous fix in the same area within ≤3 days.

**File hotspots (fix-like commits with real, non-stamp diffs).**

| File | Size | Fixes | Of which |
|---|---:|---:|---|
| `store.rs` | 48,808 lines | **101** | 49 U2, 23 AUTH, 10 CMD |
| `rxdb_peer.rs` | 22,351 lines | 36 | |
| `app.js` | | 34 | 17 U1 |
| `command_plane.rs` | | 31 | 11 GEN, 10 CMD |
| `connection_handler_rs.rs` | | 22 | 12 U4 |
| `sync.js` | | 27 | |

## 4. RFC U1–U5 assessment

| RFC cause | Verdict | Evidence (fixes) | Required change to the RFC |
|---|---|---|---|
| U1 State contract | Correct, too narrow | 31 (+12 QRY, +4 MTAB, +35 app-side) | Contract must fix read path, readiness, push/invalidation, eviction, checkpoint and follower behavior per profile; tri-state known/unknown/stale; maintenance ack per needed collection |
| U2 Stores/projections | Correct, largest | 59 (+9 SQL, ~5 CMD) | Name the real fault: no per-field write ownership, shadow/compat tables as read fallbacks, projections writing RxDB tables directly, per-request SQLite connections/deferred transactions |
| U3 Data model | Correct, over-weighted | 13 | Byte budgets, not counts; control-plane retention; note that U3 fixes inside the projection layer caused U2 regressions |
| U4 Transport | Partly | 24 (HOL ≈ 6) | Add the unspecified session/responder state machine and the inconsistent admission/flow-control protocol |
| U5 Gates/parallel changers | Correct | 22 (+17 OBS) | Add instrument validation (broken counters, wrong SQL timestamp comparison); name hand-edited global singletons as conflict magnets |
| — | **Missing** | AUTH 43, ASSET 36, GEN 29, LIFE 28, CMD 18, OBS 17, REPL 8, SCHEMA 6, IDB 6 | Add as U6 authority model, U7 asset/contract generation, U8 lifecycle/ownership scopes, U9 command lifecycle; REPL fits in a sharpened U4 |

**Additional structural finding.** The sync core has no extension boundary for domain features.
206 feature commits (Workjet Supervisor, JourFix, KPI, federation, guest frames) edit the same
files: `command_plane.rs` inventory, `index_mod.rs`/`connection_handler_rs.rs` guarded auxiliary
handlers, and `store.rs`. Domain incident notes also live in `ctox-rxdb.md` §0–990 (D&B
classification, Outbound receipts, JourFix narration). This is a driver of U5 and GEN.

## 5. Ranking and recommendation

Score = fixes × (1 + share of the class's fixes that sit in a re-fixed sub-pattern of ≥3). The
alternative score, n × average sub-pattern size, ranks ASSET first and leaves the rest of the top
five unchanged.

| Rank | Class | n | Re-fix share | Score |
|---:|---|---:|---:|---:|
| 1 | U2 | 59 | 0.68 | 99 |
| 2 | AUTH | 43 | 0.58 | 68 |
| 3 | ASSET | 36 | 0.86 | 67 |
| 4 | U1 | 31 | 0.71 | 53 (with QRY+MTAB: 47 fixes, score ≈75) |
| 5 | GEN | 29 | 0.72 | 50 |
| 6 | LIFE / U5 | 28 / 22 | 0.36 / 0.73 | 38 / 38 |
| 8 | U4 | 24 | 0.29 | 31 |
| 9 | CMD | 18 | 0.17 | 21 |
| 10 | OBS | 17 | 0.18 | 20 |

Recommended architecture changes, by share of future fixes removed (share = this class's past
fixes, used as an estimate):

1. **One owner per fact and one projection writer.** Declare ownership per collection and field
   in the generated schema contract. The native peer rejects non-owner writes terminally; the
   browser never writes server-owned fields. A single batched projection writer is the only
   writer of RxDB tables and goes through the RxDB document API. Retire the shadow and compat
   tables as read sources. Use one long-lived writer connection with IMMEDIATE transactions.
   **This removes U2 + SQL + ~5 CMD ≈ 73 fixes (≈20%)** and chains 1, 4 and 5, and it shrinks the
   `store.rs` hotspot. It is the biggest lever and also the most expensive.
2. **An explicit profile contract** (RFC §3.1 extended to the six behaviors, with tri-state
   values and a static guard against apps reading `phase`/`readiness`). It removes U1 + QRY +
   MTAB ≈ 47 (≈13%) plus most of the 35 app-side compensation commits, and it is the
   highest-impact class for users.
3. **A capability-bound session context.** Authority is resolved once per connection generation
   into an immutable context. Handlers and windows declare their required scope, the framework
   checks it at admission and at publication, and cache keys include the permission digest. No
   process-wide fences. It removes AUTH ≈ 43 (≈12%; ≈16% with the hardening commits classed as
   features).
4. **Quick, mechanical wins:**
   - A build-time content-hashed module graph or import map replaces every `?v=`/generation
     string. Removes ASSET 36 + 17 bumps (≈10% plus the stamp noise in ~130 fixes).
   - Generate the command inventory, allowlists and schema representations from one registry.
     Removes GEN 29 (≈8%).
   - Together that is ≈18% of fixes with low risk. It also removes the two biggest merge-conflict
     magnets and so reduces U5.
5. **Lifecycle and transport.** Structured concurrency, where every slot, lease, observer and
   timer is owned by the connection generation or view scope; plus a specified WebRTC
   session/responder state machine with a single admission/flow-control protocol and separate
   lanes. Removes LIFE + U4 + REPL ≈ 60 (≈16%).

**Order.** Take step 4 first (cheap; cuts U5 conflicts). Then do steps 2 and 1 together under the
RFC's strangler approach: the profile contract defines which path owns writes per collection, so
the ownership work can migrate collection by collection. Keep step 3 next to step 1, because
authority and ownership are both per-collection declarations in the same contract. Every stage
needs validated instruments (OBS) before its gates mean anything.

## 6. Uncertainty

- **Single-person classification.** 366 fix-like commits were classified by one analyst, mostly
  from subject and file evidence; 274 of them (75%) have empty bodies. I estimate ±15–20% per
  class. The boundaries between AUTH and LIFE, U2 and CMD, and U1 and QRY are fuzzy for roughly 40
  commits.
- **Feature vs fix.** About 22 Codex "Bind/Require/Fence/Verify …" commits are classed as
  features but could count as AUTH hardening.
- **Sub-pattern granularity is my choice.** It affects the recurrence score, not the class counts.
- **UNCLEAR: 1** (`5e81f8e65`). The `XDOM` classification of 52 commits (crew UI, importer,
  Outbound domain, grok) is defensible but could move a few into U1 or U2.
- **Coverage gap.** App-side compensation outside `commits.txt` (35 commits) is counted only as a
  note, not classified.
