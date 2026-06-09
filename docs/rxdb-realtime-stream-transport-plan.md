# Implementation Plan — Constant Real-Time RxDB/WebRTC Stream + App-Transparent Transport

Status: SUPERSEDED / LARGELY IMPLEMENTED — kept as history. The phases below
landed across the Phase 1-4 transport commits and the 2026-06 stability work;
the "current state" claims and line numbers in this document are STALE. The
canonical, code-verified description of the data plane is `docs/ctox-rxdb.md`.
Do not implement against this plan.

Original status line: PROPOSAL (no code touched yet). Author: engine review.
Scope: the hard-forked RxDB-WebRTC layer — browser (`src/apps/business-os/rxdb/src/`) and native (`src/core/rxdb/src/plugins/replication_webrtc/`, `src/core/business_os/rxdb_peer.rs`).

## 0. Why (the two themes, in the operator's words)

1. **Constant real-time stream is the crux, not quantity.** WebRTC DataChannels are built for many small, steady packets. A single oversized / bursted payload overruns the SCTP send buffer and **the browser kills the channel**. STUN/few streams are fine; an un-paced large transfer is not.
2. **Apps must only write to RxDB and read reactively.** Everything else — chunking, pacing, backpressure, priority of the currently-queried collection, on-demand streaming of large content, eviction — must live **inside the RxDB-WebRTC layer**, transparent to apps. Today this is violated: orchestration lives in `app.js`.

### Acceptance criteria (measurable, gate the whole effort)
- A1. From a **truly clean browser session** (fresh IndexedDB / incognito), opening any of the ~20 apps shows that app's data within a bounded time (target: first useful rows ≤ 3 s, full active-collection initial replication ≤ 10 s on a typical link), with **zero DataChannel `close`/`error` from buffer overrun**.
- A2. Replicating a large collection (`documents` + `document_blob_chunks`) **never** kills the channel and **never** stalls the small/foreground collections beyond a bounded fairness window (target: foreground collection p95 frame latency ≤ 250 ms while a 8 MB transfer runs in the background).
- A3. No business-os **app module code** contains sync orchestration (no `startModuleSync`/warmup/deferral logic in `app.js`); apps call only RxDB `insert()/find().$`.
- A4. Every hypothesis below is backed by an automated **e2e test** (real browser ↔ real native peer) **and** a **benchmark** with explicit pass/fail thresholds, runnable in CI and locally.

## 1. Current state (code-grounded baseline)

- One signaling room + one `RTCPeerConnection` + one DataChannel **per collection** (~88 collections): browser `replication-webrtc.mjs:99-101`, `shared/sync.js:791`; native `rxdb_peer.rs:929-961`, `index_mod.rs:314-337`.
- **Native backpressure is dead:** `WebRTCRsConnectionHandler` does not override `buffered_bytes` → inherits default `0` (`webrtc_types.rs:107-109`). The high-water checks in `query_fetch_handler.rs:676-706`, `file_fetch_handler.rs:342-351,449-456` therefore never fire. Only a `SEND_FRAME_PAUSE = 1ms` per chunk + ack-window stop-and-wait throttle (`connection_handler_rs.rs:737-872`). → native can burst.
- Browser backpressure works (`webrtc-native.mjs:371-390` `waitForSendBuffer`, ack window `:272-295`).
- Transport chunk layer is **collection-agnostic** (keyed by `transferId`/peer) — `connection_handler_rs.rs` and `webrtc-native.mjs` framing. Chunk constants: `MAX_INLINE_FRAME_BYTES=14336`, `MAX_CHUNK_BYTES=10240`, `MAX_TRANSFER_BYTES=8 MiB` (`frame_contract_generated.rs:4-9`).
- Plain replication frames carry **no collection id** (`webrtc_types.rs:29-47`); `masterChangeStream$` uses a fixed response id that would collide if multiplexed (`index_mod.rs:640-647`, `:1023-1029`). Demand RPCs already carry `collectionName` in-band (`query_fetch_handler.rs:53`, `file_fetch_handler.rs:34`).
- Priority today = (a) connection-admission (critical-first + preempt, `webrtc-native.mjs:1404-1648`) which only works because each collection is its own connection, and (b) per-connection frame-kind priority (`:1817-1848`), **not collection-aware**.
- App-level orchestration: `app.js:29-52` (critical list), `:1933-2000` (warmup), `:3571-3697` (startModuleSync/defer/background).
- **Dormant infra:** demand-fetch sources never registered in `rxdb_peer.rs` (file fetch ⇒ `FILE_NOT_FOUND`); eviction `budgetBytes` defaults to 0 (no setter) → eviction effectively off.

## 2. Hypotheses (each gated by an e2e test + a benchmark)

- **H1 (root cause).** The native peer bursts large transfers without backpressure, overrunning the browser DataChannel buffer → the browser closes the channel → large collections (`documents`/blob chunks) fail to replicate while small ones succeed.
- **H2.** Implementing real native backpressure (`buffered_bytes` + pacing on `bufferedAmount`) and hard-capping per-message size keeps the channel open and lets `documents` complete initial replication from a clean session.
- **H3.** Replacing N per-collection connections with **one multiplexed stream** (one room/PC/DataChannel, collection-tagged frames) is at least as reliable and reduces warm-up time, with STUN-only.
- **H4.** **Collection-aware in-stream priority** driven by the app's reactive subscriptions (not by `app.js`) serves the foreground collection's data first under contention.
- **H5.** **Demand-load + eviction** (activated, transparent in RxDB) keep memory bounded and the stream real-time for arbitrarily large content, with apps unaware.

## 3. Phases

> Each phase lands behind a test + benchmark proving its hypothesis before the next phase starts. We do NOT claim a phase done without its green e2e + benchmark.

### Phase 0 — Repro harness + instrumentation (proves H1, baselines everything)
- Extend `src/core/rxdb/tools/browser_rust_smoke.js` into a deterministic **clean-session** harness: launch real native peer + headless Chrome with a **fresh profile** (empty IndexedDB), seed the native store with a fixed corpus (incl. a large `documents`/`document_blob_chunks` payload), open a module, and assert per-collection initial-replication completion.
- Add instrumentation surfaced in `transportStatus$`: DataChannel `close`/`error` events with reason, `bufferedAmount` time-series, per-collection initial-replication start/complete timestamps, SCTP send failures. Native: log `buffered_amount()` from webrtc-rs per send.
- **Benchmark B0 (baseline):** measure today — time-to-first-rows and time-to-complete per collection; count DataChannel kills; record `bufferedAmount` peak. Expected: `documents` kills/stalls; small collections complete. This *proves H1* by reproducing the operator's incognito result deterministically and attributing it to buffer overrun / channel close.
- Artifacts: `tools/clean_session_e2e.js`, `tools/transport_bench.js` (JSON results + thresholds).

### Phase 1 — Theme A: guarantee the constant real-time stream
Changes:
- **Native `buffered_bytes`:** implement on `WebRTCRsConnectionHandler` to return the live webrtc-rs DataChannel `buffered_amount()` per peer (`connection_handler_rs.rs`; remove reliance on the `0` default in `webrtc_types.rs:107-109`). This activates the existing high-water guards in `query_fetch_handler.rs:676-706` and `file_fetch_handler.rs:342-351,449-456`.
- **Pacing both directions:** before every chunk send (native `connection_handler_rs.rs:737-872`; browser `webrtc-native.mjs:371-390`), block until `bufferedAmount ≤ LOW_WATER`. Replace the fixed `SEND_FRAME_PAUSE=1ms` with watermark-driven pacing. Tune `SEND_BUFFER_HIGH/LOW_WATER` to keep p95 real-time latency under target while sustaining throughput.
- **Hard size invariant:** assert no single DataChannel message exceeds the negotiated SCTP max message size; everything above `MAX_INLINE_FRAME_BYTES` MUST go through the chunk path. Add a guard that refuses/levels any oversized send (both sides) and a test that fuzzes message sizes.
- **Fairness yield:** generalize the browser's `drainHighPriorityInlineFrames` interleave (`webrtc-native.mjs:314-327`) so a long transfer yields between windows — prerequisite for A2 even before multiplexing.
- **Hypothesis tested:** H2.
- **E2E test E1:** clean-session harness replicates the large `documents` corpus → assert channel never closes, `documents` reaches `initialReplicationState=complete`, `count()==N`.
- **Benchmark B1:** large-transfer run — assert 0 channel kills, `bufferedAmount` peak ≤ HIGH_WATER + margin, foreground-collection p95 frame latency ≤ 250 ms during the transfer (A2), throughput ≥ baseline. Gate: B1 green vs B0 red on the kills metric.

### Phase 2 — Theme B (1/2): move priority/orchestration INTO RxDB
Changes:
- **Subscription-driven priority:** the RxDB layer derives "which collection is foreground" from active reactive queries (`rx-database.mjs` query/subscription lifecycle), not from `app.js`. A collection with a live subscription / recent `.exec()` is High; others Normal/Low.
- **Collection-aware send queue:** add a collection dimension to the priority queue (`webrtc-native.mjs:1817-1848` and native `PeerSendQueue` `connection_handler_rs.rs:131-153`) so foreground-collection frames jump ahead, replacing the connection-admission priority that disappears under multiplexing.
- **Remove app orchestration:** delete `startModuleSync`/`runCriticalSyncWarmup`/`deferredSyncModules`/`backgroundModuleWork` from `app.js:29-52,1933-2000,3571-3697`; apps just read/write. RxDB starts/prioritizes replication lazily on first subscription.
- **Hypothesis tested:** H4 (+ A3).
- **E2E test E2:** open app B while app A's large transfer runs → assert app B's foreground collection delivers first rows ≤ 3 s (priority honored) with no app-level scheduling present. Add a guard test that fails if `app.js` reintroduces sync orchestration (static scan).
- **Benchmark B2:** contention scenario — foreground time-to-first-rows with/without a competing background transfer; assert priority inversion bounded.

### Phase 3 — Theme A/B: multiplex to one constant stream
Changes (the consolidation, now safe because Phase 1 guarantees real-time and Phase 2 moved priority in-stream):
- **Wire contract:** add a `collection`/`topic` field to `WebRTCMessage`/`WebRTCResponse` (`webrtc_types.rs:29-47`) and qualify the `masterChangeStream$` id per collection (`index_mod.rs:640-647,1023-1029`). Bump protocol version; keep a compatibility path for one release.
- **One room + one handler + demux:** browser opens a single `CtoxWebRtcNativePeer` for the whole `sync_room` (not per collection) — refactor `replication-webrtc.mjs:99`, `shared/sync.js:791`; native consolidates the 88 `SignalingClient`/handler/pool into one room + one `WebRTCRsConnectionHandler` + one demultiplexing `message_stream` loop (`rxdb_peer.rs:929-961`, `index_mod.rs:314-489`). Per-collection master handler + fork state become collection-keyed maps.
- Retire the connection pool/cap/gate (`MAX_GLOBAL_RTC_PEER_CONNECTIONS`, `criticalRequested`, preemption) — obsolete once there is one connection. Remove the LimitNOFILE workaround dependency (FD usage collapses).
- **Hypothesis tested:** H3.
- **E2E test E3:** full clean-session matrix over all ~20 apps → each app's data renders; one signaling room, one PC observed.
- **Benchmark B3:** warm-up time (all collections ready) single-stream vs per-collection baseline; FD count; signaling socket count. Gate: ≤ baseline warm-up, 1 PC, FDs bounded.

### Phase 4 — Theme B (2/2): activate demand-load + eviction (large content on demand)
Changes:
- **Register demand sources** in `rxdb_peer.rs` (`register_stream_source`/`register_source`/`set_auth_check`) so `rxdb.file.fetch`/`rxdb.query.fetch` actually serve (today only the auto-registered query collection works; file ⇒ `FILE_NOT_FOUND`).
- **Set a memory budget** (`query-meta-storage.mjs setBudgetBytes`) and a **global** budget across collections; verify eviction deletes from the primary store under pressure.
- Route large blobs through demand-fetch (lazy on reactive read) instead of eager `desktop_file_chunks`-style push where appropriate.
- **Hypothesis tested:** H5.
- **E2E test E4:** open a doc with large blobs → only requested ranges fetched (assert bytes-on-wire ≪ total), memory stays under budget, eviction frees primary store; reopen resumes via `knownSequences` without re-fetch.
- **Benchmark B4:** memory ceiling under a corpus far larger than budget; bytes-on-wire for scroll/open patterns; assert bounded.

## 4. Test & benchmark infrastructure (the non-negotiable)

- **Real e2e only** (per CLAUDE.md/HARNESS discipline): real native Rust peer + real headless Chrome over real WebRTC/signaling. Extend the existing `src/core/rxdb/tools/browser_rust_smoke*.js` / `browser_rust_soak.js` harnesses; add `clean_session_e2e.js` (fresh-profile matrix over all apps) and `transport_bench.js`.
- **Clean-session guarantee:** every e2e starts from an empty IndexedDB (fresh Chrome profile) — this is the gap that hid the bug; cached IndexedDB must never mask a result.
- **Metrics captured per run (JSON, thresholded):** DataChannel close/error count (must be 0), `bufferedAmount` peak, per-collection time-to-first-row + time-to-complete, foreground p95 frame latency under background load, bytes-on-wire, peak memory, FD count, signaling socket count.
- **CI gates:** Phase N cannot merge unless its EN test + BN benchmark are green and the regression (B0) is reproduced red without the fix. Add to `.github/workflows/ci.yml` next to the existing rxdb smoke gates.
- **Soak:** extend `browser_rust_soak.js` to run the multiplexed stream for ≥ 30 min with periodic large transfers; assert no channel kills, bounded memory, stable latency.

## 5. Sequencing, risk, rollback

- Order: **Phase 0 → 1 → 2 → 3 → 4.** Phase 1 alone may already satisfy A1/A2 for `documents` (test it before committing to the full multiplex). Phase 3 (multiplex) is the largest change and is gated on Phases 1–2 being green.
- Each phase is independently shippable and reversible (protocol-version gated for Phase 3).
- Risk hotspots: SCTP max-message-size negotiation differences across browsers; fairness starvation under multiplex (mitigated by Phase 2 priority + Phase 1 yield); checkpoint/fork-state correctness when collection-keyed (covered by E3 + existing replication tests).
- Rollback: keep per-collection path behind the protocol version for one release; flip via the RxDB layer, not per-app.

## 6. Definition of done
- A1–A4 met, all of E1–E4 + B1–B4 green in CI, B0 regression reproduced, soak clean, and a clean-incognito walkthrough of all ~20 apps verified by screenshot on a live instance (skf first, then cto1.kunstmen once its shell deploy lands).
