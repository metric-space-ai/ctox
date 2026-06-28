# CTOX DB (ctox-rxdb) — The Business OS Data Plane

This is the reference document for CTOX DB: the WebRTC-only replication layer
between the browser-side Business OS shell and the CTOX daemon. It is written
for engineers and coding agents, and every technical claim in it has been
verified against the cited source file. When this document and the code
disagree, the code wins — and this document should be fixed.

Two implementations, one contract:

| Side | Name | Location |
|---|---|---|
| Browser | `ctox-rxdb-js` (public name **CTOX DB**) | `src/apps/business-os/rxdb/` |
| Daemon | `rxdb-rs` (crate `ctox-rxdb`, lib name `rxdb`) | `src/core/rxdb/` + `src/core/business_os/rxdb_peer.rs` |

---

## 1. What CTOX DB is

CTOX DB is a CTOX-owned data-plane runtime *derived from* RxDB concepts. It is
**not upstream RxDB** and not a drop-in replacement for the npm `rxdb`
package (`src/apps/business-os/rxdb/README.md`).

The identity contract is pinned in `src/apps/business-os/rxdb/manifest.json`:

| Field | Value |
|---|---|
| `name` (runtime id) | `ctox-rxdb-js` |
| `public_name` | `CTOX DB` |
| `format` | `browser-esm` |
| `package_manager` | `none` |
| `api_contract` | `ctox-db-business-os-v1` |
| `upstream_compatibility` | `not-upstream-rxdb` |
| `protocol` | `ctox-rxdb-protocol-v1` |
| `entry` | `dist/ctox-rxdb-js.mjs` |
| `storage` / `transport` | `indexeddb-native` / `webrtc-native` |
| `contracts` | `ctox-rxdb-protocol-v1`, `ctox-schema-hash-v1`, `ctox-peer-session-v1`, `ctox-checkpoint-epoch-v1` |

Consequences (all from `src/apps/business-os/rxdb/README.md`):

- Apps must not import `rxdb` or `rxdb/plugins/...`. They receive database and
  collection handles from the Business OS runtime (`shared/db.js`).
- Plain browser ESM: no install step, no lockfile, no vendored dependency
  tree, no third-party peer packages. Native `indexedDB`, native
  `RTCPeerConnection`, native `WebSocket`.
- No feature gates, paid-tier checks, or runtime add-on unlocks.
  `addRxPlugin()` exists only as a transition shim for old bootstrap code.

The Rust side is a byte-correct port of RxDB 16.20.0 (upstream pin
`c69c94bb…`, see `src/core/rxdb/PORTING.md` and `vendor/rxdb.version`),
reduced to the CTOX-as-WebRTC-peer scope. Root `README.md` ("Business OS
Connectivity", from line 54) defines the relationship: the browser shell may
be delivered by CTOX itself, ctox.dev, or the desktop app, but business data
always uses one path — CTOX DB over WebRTC between browser IndexedDB and the
CTOX SQLite store.

---

## 2. The Data Boundary (normative)

Root `README.md:165-176` ("### Data Boundary") is the normative statement.
Verbatim:

> The following records must never be proxied through HTTP between the
> browser and CTOX:
>
> - Business OS collections and module runtime data
> - `business_commands` and `ctox_queue_tasks`
> - `desktop_files` and `desktop_file_chunks`
> - module manifests and native runtime status
>
> Those records replicate only through RxDB/WebRTC and persist on the CTOX
> side in `runtime/ctox.sqlite3`.

(On the exact SQLite file see the persistence map in §4 — the document store
is `runtime/business-os-rxdb.sqlite3`; the README sentence is a boundary-level
simplification, not a path spec.)

HTTP is **delivery and bootstrap only**: static shell assets, launch context,
packed `ctox_config`, `/.well-known/ctox-business-os.json` status. In managed
mode that well-known file must keep `httpDataProxy:false` and
`businessDataPath:"rxdb-webrtc"` (`README.md:157-158`). The daemon's sync
config hard-codes `http_bridge_available: false` and `transport: "webrtc"`
(`src/core/business_os/store.rs::sync_config`). The browser runtime refuses to
start without a WebRTC-capable sync contract — `createSyncRuntime` throws
`"Business OS requires RxDB WebRTC sync; unsupported sync contract."`
(`src/apps/business-os/shared/sync.js:26-29`).

**Any HTTP fallback for these records is a regression, not a feature.**

This is mechanically enforced by
`src/apps/business-os/rxdb/tests/data-plane-guard-smoke.mjs`, a ratchet guard
whose allowlist may only change with an explicit architecture decision
recorded in this document.

---

## 3. Architecture — browser side

### 3.1 Module map (`src/apps/business-os/rxdb/src/`)

All descriptions verified against the file headers.

| Module | One line |
|---|---|
| `index.mjs` | Public browser-ESM entry; everything the bundle exports. |
| `rx-database.mjs` | `createRxDatabase` / collection surface of CTOX DB. |
| `schema.mjs` | Canonical JSON + WebCrypto SHA-256 schema hashes; schema-hash registry; protocol payload helpers. |
| `storage-indexeddb.mjs` | Minimal document storage over native IndexedDB (default db `ctox_business_os_js_v1`). |
| `replication-webrtc.mjs` | `replicateWebRTC`: shared room peer plus per-collection replication states (pull/push, checkpoints, master handler). |
| `webrtc-native.mjs` | Native `RTCPeerConnection`/`WebSocket` peer: signaling, framed transport, prioritised send queue, request/response RPC. |
| `active-collections.mjs` | Registry of "active" (foreground) collections, derived from real subscriptions and recent `.exec()` reads; feeds the `rxdb.activeCollections` control frame. |
| `frame-contract.generated.mjs` | GENERATED frame-protocol constants (do not edit; see §7). |
| `protocol-contract.generated.mjs` | GENERATED protocol/capability/error-code constants (do not edit; see §7). |
| `demand-loading-transport.mjs` | Turns the bidirectional `peer.request` channel into a `rxdb.query.fetch` / `rxdb.file.fetch` request/response layer, correlating pushed chunk frames. |
| `query-demand-loader.mjs` | Sits between `RxQuery.exec` and storage; fetches missing (collection, fingerprint, window) data over WebRTC, with in-flight dedup. |
| `file-demand-loader.mjs` | Streams large files in chunks. Chunk collections can persist fetched chunk presence; metadata collections such as `desktop_files` use the same RPC without writing binary chunks into the metadata store. |
| `chunk-decoder.mjs` | V1.5 chunk envelope decoder (plain and deflate-raw via native `DecompressionStream`). |
| `query-fingerprint.mjs` | Canonical SHA-256 query fingerprint, byte-identical between JS and Rust (corpus-verified). |
| `query-meta-storage.mjs` | V1.5 sidecar metadata store (`ctox_business_os_v1_5_meta`): query-window completeness, access times, cache stats. Separate from the primary store. |
| `query-meta-backend-indexeddb.mjs` | Lazy-open IndexedDB backend for the sidecar. |
| `query-meta-backend-memory.mjs` | In-memory sidecar backend (Node tests, fallback). |
| `multi-tab-broker.mjs` | BroadcastChannel leader election: one tab per (databaseName, windowKey) does the remote fetch; others subscribe. |
| `advanced-status-bridge.mjs` | Folds V1.5 health into the `business-os-advanced-status-v1` envelope the UI/smoke harness consumes. |
| `v1_5_status.mjs` | V1.5 status field surface (pinned field list). |
| `observable.mjs` | Minimal subject/behaviour-subject. |
| `event-target.mjs` | Tiny `EventTarget`-backed emitter. |

### 3.2 Shell integration

**`shared/db.js` — `createBusinessDb({ name })`.** Imports the bundle from
`../rxdb/dist/ctox-rxdb-js.mjs?v=<buster>` (timeout-guarded, with one
cache-busted retry), runs an IndexedDB preflight probe, then
`createRxDatabase` with `getCtoxIndexedDbStorage()`. If the primary IndexedDB
stays blocked it opens a session-pinned recovery database
(`${name}__recovery_<ts>`). It returns a façade (`addCollections`,
`collection`, `close`, `runtime` identity tag `CTOX_RXDB_RUNTIME`).

**`shared/sync.js` — `createSyncRuntime({ db, config })`.** WebRTC-only (see
§2). One bridge per collection via `startWebRtcReplication`, started serially
through `collectionStartQueue` (500 ms spacing). Key mechanics, all in
`sync.js`:

- *Per-collection bridge:* `rxdb.replicateWebRTC({ collection, topic: room,
  … })` — note the topic is the **bare sync room**; the per-collection
  `collectionTopic(...)` survives only as a diagnostics label (Phase 3
  multiplex, `sync.js:504-511`).
- *Error classification chain* (`error$` subscriber, in this order — the
  order is load-bearing): signaling control-plane error → schema/protocol
  error → replication-IO error → transient shutdown event → peer lifecycle
  event → transient signaling-socket blip → generic error. The transient-blip
  branch exists because the generic fallthrough used to turn every Wi-Fi blip
  into a mass hard-restart across ~80 collections (`sync.js:693-710`).
- *Restart sweep:* `scheduleRestartOfUnhealthyCollections` re-runs every 5 s
  while any collection reports `reconnecting/failed/error/stopped`; an
  `online` listener triggers it at 250 ms. `restartCollections` also enforces
  a 30 s native-peer-open deadline with one retry.
- *Watchdog:* a 30 s `nativePeerOpenWatchdog` per bridge escalates to
  `onFatalPeerError` if no native DataChannel opens.
- *Suspend/resume:* `suspendCollections`/`resumeCollections` park bridges as
  `paused` without tearing the runtime down.
- *Checkpoint handshake evidence:* a peer only counts as protocol-ready when
  it advertises `ctox-peer-session-v1`, `ctox-checkpoint-epoch-v1`, and an
  `advertised` checkpoint with a non-empty `epoch`
  (`hasNativePeerProtocolEvidence`, `classifyCheckpointProtocolError`).

`shared/sync-contract.js` supplies `collectionTopic`, `batchSizeFor`
(chunk-ish collections get small batches), and the `nativeRxdbPeerReady` gate.

Runtime-installed Business OS app collections are dynamic but still part of
the CTOX DB data plane. The browser registers them from each module's
`schema.js`; the native peer registers matching schemas from
`runtime/business-os/installed-modules/<module-id>/collections.schema.json`
when it starts. App creation finalization refreshes a running in-process native
peer so newly validated app collections are immediately available over WebRTC.
The three declarations must agree: `module.json` `collections`,
`collections.schema.json`, and `schema.js`.

---

## 4. Architecture — native side

### 4.1 Crate layout (`src/core/rxdb/`)

Standalone Cargo package `ctox-rxdb` (lib name `rxdb`), with its own
`Cargo.toml` and `Cargo.lock`. The root `Cargo.toml` has **no `[workspace]`
section**; the crate is consumed as a path dependency
(`rxdb = { package = "ctox-rxdb", path = "src/core/rxdb" }`), so its tests run
only via `--manifest-path` (see §10).

- `src/storage/sqlite/` — SQLite storage backend (`cleanup.rs`,
  `index_mod.rs`, `instance.rs`, `sql.rs`, `types.rs`): RxDB document JSON
  stored unchanged, with indexed metadata columns for PK lookup, cleanup and
  checkpoint scans.
- `src/plugins/replication/` — the generic replication protocol port
  (`index_mod.rs`, `replication_helper.rs`) used by fork states.
- `src/plugins/replication_webrtc/` — the WebRTC transport, 12 modules:

| Module | One line |
|---|---|
| `mod.rs` | Public re-exports for the plugin. |
| `index_mod.rs` | `replicate_web_rtc*` entry points, room-level handshake, master/fork election, per-collection relays and fork states, `RxWebRTCReplicationPool`. |
| `connection_handler_rs.rs` | webrtc-rs connection handler: peer build, send queue, framed transport, backpressure, active-collections priority. |
| `signaling_client.rs` | WebSocket signaling client with reconnect supervisor, keepalive, peer-role map, URL provider. |
| `signaling_protocol.rs` | Wire types for the simple-peer signaling protocol (+ CTOX `ctoxError`, peer descriptors). |
| `webrtc_helper.rs` | Master election hash + `send_message_and_await_answer` (60 s deadline, disconnect race). |
| `webrtc_types.rs` | `WebRTCMessage`/`WebRTCResponse`/`WebRTCWireFrame`, the `WebRTCConnectionHandler` trait (incl. `close_peer`). |
| `query_fetch_handler.rs` | Server-push dispatcher for `rxdb.query.fetch` → `rxdb.query.chunk` streams (+ cancel). |
| `file_fetch_handler.rs` | Same for `rxdb.file.fetch` (base64 chunks, range resume). |
| `frame_contract_generated.rs` | GENERATED frame constants (see §7). |
| `protocol_contract_generated.rs` | GENERATED protocol constants (see §7). |
| `v1_5_status.rs` | Rust mirror of the V1.5 status field surface. |

Also in the crate: `examples/v15_wire_daemon.rs` (stdio wire daemon for
cross-process JS↔Rust tests) and `examples/v15_scale_wire_loop.rs`, `tools/`
(contract generators, smoke/soak drivers), `tests/` (conformance + fixtures),
`vendor/` (upstream snapshot), `PORTING.md` + `revisions/` (port ledger).

### 4.2 The native peer (`src/core/business_os/rxdb_peer.rs`)

`spawn_native_peer` starts one supervised OS thread (`business-os-rxdb-peer`):

- **Supervised respawn loop.** Every non-intentional exit respawns with
  capped exponential backoff (5 s → 300 s); a run that survived ≥ 600 s
  resets the backoff. The sync config is **re-read per attempt**, so room
  password rotation and signaling changes reach a respawned peer without a
  daemon restart. The peer previously died permanently on any exit — a boot
  race against the signaling server cost the whole daemon lifetime of sync.
- **`run_native_peer`** acquires a process lock
  (`runtime/business-os-rxdb-peer.lock`), starts the status heartbeat on a
  dedicated thread *before* bring-up (`runtime/business-os-rxdb-peer.status.json`,
  written every 5 s, TTL 30 s), registers collections fault-tolerantly
  (failing *required* collections abort the run; optional ones are skipped),
  then brings up **one** multiplexed replication session for the whole sync
  room via `replicate_web_rtc_rs_multi_with_url_provider`.
- **Bring-up failure is fatal, not a zombie.** A 20 s bring-up timeout aborts
  the attempt (the in-flight task is `abort()`ed so it cannot leak a live
  orphan session) and returns an error to the supervisor for a backed-off
  respawn. The previous log-and-continue behaviour produced the canonical
  zombie: heartbeat "running", zero replication, no retry.
- **Heartbeat watchdog.** A 15 s watchdog inside the run checks the peer's
  own heartbeat file; staleness above 90 s forces a clean shutdown
  (`NativePeerExit::WatchdogStale`) so the supervisor can respawn and the
  process lock is released.
- **`replicationUp`.** `NATIVE_PEER_REPLICATION_UP` is set only after the
  multiplexed session is up and cleared on exit; the heartbeat/status surface
  reports it, so "process alive but not replicating" is observable.

The same file hosts the background projection loops (commands, notes, desktop
file index, channel state, users, runtime settings, module catalog, ticket
state, knowledge tables, business-record projections) that write core daemon
state into the RxDB store for replication.

### 4.3 Persistence map

| Store | Where | What |
|---|---|---|
| Browser primary | IndexedDB `ctox_business_os_js_v1` (`storage-indexeddb.mjs`) | Replicated documents per collection plus local browser performance indexes such as `schemaIndexEntries` and `collectionPushableLwtId`. |
| Browser sidecar | IndexedDB `ctox_business_os_v1_5_meta` (`query-meta-storage.mjs`) | V1.5 demand-loading metadata only; query/cache metadata stays out of the primary document store. |
| Native RxDB store | **`runtime/business-os-rxdb.sqlite3`** (`store.rs::RXDB_STORE_FILE`, `rxdb_store_path`) | The rxdb-rs document store (internal RxDB database name `ctox_business_os`, `rxdb_peer.rs::RXDB_SQLITE_DATABASE_NAME`). |
| Core runtime store | `runtime/ctox.sqlite3` | The daemon's unified store (queue, tickets, settings, engine tables). Not the RxDB document store; the projection loops read it and write into the RxDB store. |
| Peer liveness | `runtime/business-os-rxdb-peer.lock`, `runtime/business-os-rxdb-peer.status.json` | Process lock + heartbeat. |

Note: root `README.md:175-176` names `runtime/ctox.sqlite3` as the
persistence target. At boundary level that is the right message (data stays
in CTOX's local SQLite, never an HTTP service); the precise file for RxDB
documents is `runtime/business-os-rxdb.sqlite3` as above.

---

## 5. Connection lifecycle (end to end)

### 5.1 Room and signaling join

- **Room derivation:** `sync_room = "ctox-business-os:{instance_id}:{room_secret_id(password)}"`
  (`store.rs::sync_config`). Both sides join this **bare room once**; all
  collections multiplex over it.
- **Signaling URL, browser:** `sync.js::signalingUrlWithBrowserMetadata` sets
  `client=ctox-business-os-browser`, `role=browser`, `instance_id`,
  `protocol=ctox-rxdb-protocol-v1`, `cap=` for each browser capability, and a
  token = first 32 chars of base64url(SHA-256(room password)) with
  `token_iat`/`token_exp` (24 h TTL).
- **Signaling URL, native:** `rxdb_peer.rs::signaling_url_with_native_metadata`
  mirrors this with `client=ctox-business-os-native`, `role=ctox_instance`
  and the same token derivation (`signaling_token_from_room_password`,
  `SIGNALING_TOKEN_TTL_SECONDS = 24h`).
- **Token freshness is re-stamped per connect attempt on BOTH sides.**
  Browser: `webrtc-native.mjs::buildSignalingUrl` rewrites
  `token_iat`/`token_exp` keeping the original TTL length on every connect.
  Native: `SignalingClient` holds a `url_provider` closure that re-derives
  the full URL — including a fresh window — on every (re)connect
  (`signaling_client.rs`, wired from `run_native_peer`). Both carry the same
  regression note: a window baked in once meant any socket drop after >24 h
  uptime became a permanent "control plane token expired" rejection loop.
- The signaling server's `joined` broadcast carries peer descriptors
  (peerId, role, protocol, …; `signaling_protocol.rs::SignalingPeerDescriptor`);
  control-plane rejections arrive as `ctoxError` frames just before the
  server closes the socket.

### 5.2 Who initiates

- **The browser initiates; the native peer is a passive responder.**
  - Browser: `webrtc-native.mjs::shouldInitiate` — `browser` toward
    `ctox_instance` ⇒ initiate; `ctox_instance` toward `browser` ⇒ never;
    otherwise lexicographic clientId tiebreak. A `forceInitiator` override
    exists for post-timeout recycles.
  - Native: the peer-list task in
    `connection_handler_rs.rs::start_signaling_tasks` deliberately registers
    **nothing** from the peer list — "the native peer must not pre-register a
    passive PeerConnection from the peer-list alone: doing so can make the
    later browser offer hit the fast path in `ensure_peer_connection` and
    never receive an answer." The responder PeerConnection is created when
    the actual offer arrives in `handle_signal`.
  - On an inbound offer, `remove_unopened_peer_before_offer` drops an
    existing peer entry whose DataChannel never opened, so a renewed browser
    offer always gets a fresh responder (glare repair).
- The browser creates the DataChannel (label `ctox-rxdb`); offer/answer/ICE
  flow over the signaling relay. Rust answers offers and adds candidates in
  `handle_signal`; per-peer builds are deduplicated via a `OnceCell` claim
  under the peers lock (`ensure_peer_connection`).

### 5.3 Handshake on DataChannel open

Room-level, two request/answer round-trips, driven by the Rust side
(`index_mod.rs::replicate_web_rtc_inner`, connect-stream task):

1. **`ctoxProtocol`** — each side sends its protocol payload: protocol id,
   capabilities, peer session (`role`, `sessionId`), representative
   collection, **per-collection schema-hash map** (`collectionSchemas`), and
   per-collection checkpoints. Validation is symmetric. Under multiplex the
   single-collection name/hash check is meaningless (representatives may
   differ); instead each collection's schema hash is validated individually
   and mismatched collections are quiesced individually — no pull/push relay
   for them, the room stays up for the rest (both sides:
   `index_mod.rs`, `replication-webrtc.mjs`). The browser additionally
   rejects peers whose `peerSession.role` is not `ctox_instance`.
2. **`token`** — each side requests the other's storage token. An **empty or
   non-string token is a handshake failure** (it corrupts the master election
   and collapses the replication identifier so distinct peers would share one
   checkpoint meta) — the Rust side errors and calls `close_peer`.

**Master/fork election (role-based first, hash second):** the native side is
master whenever the remote role is `browser`; only for non-browser peers does
the deterministic hash election apply
(`is_master_in_webrtc_replication`: both peers compare
`H(own + "|" + other)` vs `H(other + "|" + own)`; larger first hash wins —
`webrtc_helper.rs`). Any handshake failure ⇒ `close_peer` so both sides
observe a disconnect and rebuild cleanly, instead of parking half-dead.

### 5.4 Per-collection replication

- **Master path (native, normally):** one master-change relay task per
  collection per peer, emitting `masterChangeStream$:{collection}` responses
  — but only while that collection is in the peer's active set
  (`is_collection_active_for_peer`, fed by `rxdb.activeCollections`).
  Method-call answers (`masterChangesSince`/`masterWrite`) are served by the
  message-stream loop, routed by the frame's `collection` field.
- **Fork path (browser, normally):** one replication state per collection,
  tunnelling collection-tagged `masterChangesSince`/`masterWrite` over the
  shared peer and filtering the collection-qualified `masterChangeStream$`
  into its pull stream. Fork pull/push errors are mirrored onto the pool
  error stream (they used to vanish unobserved). Handshake tasks are tracked
  on the **peer**, so a peer drop mid-handshake aborts them instead of
  letting a late completion register relays for a dead peer.

---

## 6. Wire protocol

### 6.1 Plain frames

JSON per DataChannel message (`webrtc_types.rs`; serde `untagged`):

```jsonc
// request                                 // response
{ "id": "...", "method": "...",            { "id": "...", "result": …,
  "params": [...],                           "error": null,
  "collection": "business_notes" }           "collection": "business_notes" }
```

`collection` is the **multiplex routing key** — optional (`None` for
handshake/control frames), so V1 single-collection peers stay
wire-compatible. Methods: `token`, `ctoxProtocol`, `masterChangesSince`,
`masterWrite`; server-push uses response id
`masterChangeStream$:{collection}` (bare `masterChangeStream$` is still
accepted from V1 peers — `webrtc-native.mjs::masterChangeStreamCollection`).

### 6.2 Control frame: active collections

`rxdb.activeCollections` with params `[[collectionName, …]]`
(`connection_handler_rs.rs::ACTIVE_COLLECTIONS_METHOD`). The native side
stores the set per peer, re-buckets everything still queued for that peer
(`PeerSendQueue::reprioritize`), and gates master-change relays on it. The
browser derives the set from real subscriptions/exec reads
(`active-collections.mjs`) and re-sends it after every completed handshake.

### 6.3 Framed transfer protocol (`ctox-rxdb-frame-v1`)

Anything above `MAX_INLINE_FRAME_BYTES` is sent as a chunked transfer with
`start` / `chunk` / `ack` / `resume` frames. Contract constants (generated,
see §7):

| Constant | Value |
|---|---|
| `MAX_INLINE_FRAME_BYTES` | 14336 |
| `MAX_CHUNK_BYTES` (JS name: `MAX_CHUNK_CHARS`) | 10240 |
| `MAX_TRANSFER_BYTES` | 8 MiB |
| `FRAME_ACK_WINDOW` | 4 |
| `MAX_FRAME_RETRIES` | 2 |

The hard invariant on both sides is `MAX_SERIALIZED_FRAME_BYTES = 16384`: a
single serialized DataChannel message above the 16 KiB SCTP ceiling gets the
channel killed by browsers. Chunks are therefore budgeted by their
**JSON-escaped byte length** — not UTF-16 char count — against that ceiling
minus the worst-case chunk-frame envelope:
`split_chunks_for_frame`/`json_escaped_char_len` in
`connection_handler_rs.rs`, mirrored by
`splitFrameChunks`/`jsonEscapedCharLen` in `webrtc-native.mjs` (which
additionally clamps to the contract's per-chunk budget). The old char-sliced
splitter let umlaut/emoji-heavy documents overrun the ceiling and silently
kill the channel; `tests/frame-chunking-smoke.mjs` and the Rust unit tests pin
the fix.

Acks ride per window (4 chunks); a missed ack triggers a `resume` probe
before the attempt is retried; receivers keep a TTL'd cache of completed-ack
state so a resume after completion gets a final ack. Flow control: browser
`bufferedAmount` watermarks 512 KiB/128 KiB; native
`OnBufferedAmountHigh`/`Low` threshold events at 1 MiB/256 KiB (webrtc-rs has
no buffered-amount getter). Sends are prioritised high/normal/low; control
frames are intrinsically high, oversized `masterWrite`s stay low, frames for
active collections are high.

### 6.4 Demand-loading RPCs (V1.5)

From `protocol_contract_generated.rs` (and the JS twin): `rxdb.query.fetch` /
`rxdb.query.chunk` / `rxdb.query.error` / `rxdb.query.cancel`, and
`rxdb.file.fetch` / `rxdb.file.chunk` / `rxdb.file.error` /
`rxdb.file.cancel`. Limits: 200 documents per chunk, 256 KiB per chunk, 8
in-flight streams, 30 s max runtime, default window limit 200. The request is
acknowledged immediately via the normal response frame; chunks are pushed
asynchronously and correlated by `requestId`
(`query_fetch_handler.rs`, `file_fetch_handler.rs`,
`demand-loading-transport.mjs`). Capability gate:
`ctox-rxdb-query-fetch-v1`.

`desktop_file_chunks` is not a normal background-pull surface for browser file
reads. The browser disables pull and query-demand loading for chunk collections;
the file viewer opens `desktop_files`, then uses `rxdb.file.fetch` against the
`desktop_files` demand source. Native CTOX serves that source from
`desktop_file_chunks`, scoped to the active `desktop_files.content_generation_id`
and loaded by deterministic chunk ids, so opening a file does not scan or
replicate the full chunk store into IndexedDB. Browser-side chunk writes (for
uploads/attachments) may still use the chunk collection push path.

---

## 7. Contracts pipeline

Wire constants exist exactly once, in fixtures, and are generated into both
sides:

```
src/core/rxdb/tests/fixtures/webrtc-frame-protocol.json
src/core/rxdb/tests/fixtures/webrtc-rxdb-protocol.json
        │
        ├─ node src/core/rxdb/tools/build_webrtc_frame_protocol_contract.mjs
        └─ node src/core/rxdb/tools/build_webrtc_rxdb_protocol_contract.mjs
        │
        ▼  four generated files (two per side):
src/apps/business-os/rxdb/src/frame-contract.generated.mjs
src/apps/business-os/rxdb/src/protocol-contract.generated.mjs
src/core/rxdb/src/plugins/replication_webrtc/frame_contract_generated.rs
src/core/rxdb/src/plugins/replication_webrtc/protocol_contract_generated.rs
```

**Rule: never hand-edit a generated file.** Change the fixture, re-run both
generators, then rebuild the browser bundle (§9) so both consumers move
together. `tests/contract-drift-smoke.mjs` re-runs the generators and fails
on any diff.

**Schema-hash registry.** `src/core/business_os/business_os_schema_hashes.json`
is the Rust-side fixture (consumed via `include_str!` in `rxdb_peer.rs`
tests) for the canonical per-collection schema hashes. The browser registry
`CTOX_BUSINESS_OS_SCHEMA_HASHES` in `schema.mjs` MUST stay identical —
enforced by `tests/schema-hash-registry-smoke.mjs`. A drifted hash silently
quiesces that collection on every peer (schema mismatch ⇒ no pull/push).
Related: `src/core/rxdb/tools/build_business_os_schema_contract.mjs` derives
the schema contract from the module `schema.js` files; the query-fingerprint
corpus under `tests/fixtures/query_fingerprint/` pins JS/Rust fingerprint
parity.

---

## 8. Failure & recovery semantics

| Failure | Mechanism | Where |
|---|---|---|
| Signaling socket drops (browser) | Self-reconnect with exponential backoff 1 s → 30 s; re-join re-broadcasts the peer list. Backoff resets on the `joined` broadcast, **not** on socket open — open-then-rejected sockets must keep backing off. | `webrtc-native.mjs::scheduleSignalingReconnect`, `handleSignalingMessage` |
| Signaling socket drops (native) | Supervisor task reconnects with 1 s → 30 s backoff using a **fresh URL from `url_provider`**, auto re-joins the room; subscribers keep observing across reconnects. Covered by a chaos test in the same file. | `signaling_client.rs` |
| Control-plane rejection | `ctoxError` frames are parsed and surfaced on both sides (the server closes the socket right after); otherwise a rejected join is indistinguishable from a blip and reconnects hammer silently. The browser shell additionally observes them via a WebSocket wrapper and treats them as fatal, non-retryable. | `signaling_client.rs`, `webrtc-native.mjs`, `sync.js::installSignalingErrorObserver` |
| Request vs disconnect race | `send_message_and_await_answer` subscribes to response **and** disconnect streams before sending and races them against a 60 s deadline; a peer dying mid-request fails the request instead of hanging the handshake/fork forever. Browser requests default to 15 s; a timed-out `ctoxProtocol`/`token` recycles the connection with `forceInitiator`. | `webrtc_helper.rs`, `webrtc-native.mjs::request` |
| Send-queue wedge | Exactly one drainer per peer queue (`draining` flag); `DrainResetGuard` re-opens the drain slot if the draining task is aborted mid-send; `remove_peer` drops the whole queue so parked senders fail fast instead of waiting on a drainer that no longer exists. | `connection_handler_rs.rs` |
| Handshake failure | `close_peer` force-closes that peer's transport so both sides observe a disconnect and rebuild cleanly — no half-dead channel-open-no-replication state. | `webrtc_types.rs::close_peer`, `index_mod.rs` |
| Reconnect resync churn | Pull/push checkpoints are retained on peer drop, keyed by a validity key = remote storage **epoch + native peer sessionId**, and re-seeded on reconnect when the key matches. A daemon restart mints a new sessionId, so the **full resync after a daemon restart is intentional** (conservative correctness); a transport blip within one daemon run resumes incrementally. | `replication-webrtc.mjs::removePeer`, `checkpointValidityKeyFromProtocol` |
| Missed trailing writes / stale pulls | Push re-run flag (`pushAgainAfterCurrent`): a local write landing during an in-flight push triggers another pass. Failed pulls/pushes re-arm single retry timers using `retryTime` (min 1 s). | `replication-webrtc.mjs` |
| Browser-side repair | Error classification (§3.2) decides between recording a reconnect hint (blips, lifecycle events) and scheduling the unhealthy-collection sweep / full restart; an `active$` drop schedules a 750 ms restart. | `sync.js` |
| Native peer death | Supervised respawn with capped backoff; config re-read per attempt (room-password rotation applies); bring-up failure or stale heartbeat ⇒ fatal exit ⇒ respawn — never a zombie. | `rxdb_peer.rs` (§4.2) |

### 8.1 Hard invariants (2026-06-10 soak campaign)

Each invariant below encodes a **real, reproduced data-loss or divergence
bug** found by driving the rxdb-soak matrix to its first fully green run
(`ok=true cycles=3 retries=0`, commit `ad81aff5`). Violating any of them
re-introduces silent data loss; every one is pinned by a regression test
that fails on the pre-fix code.

| Invariant | Bug it encodes (symptom) | Enforced in | Pinned by |
|---|---|---|---|
| **lwt stamping and storage commit are atomic** under `locked_run`. Never stamp `_meta.lwt` before taking the database lock. | Concurrent writers committed out of lwt order; a pull reading in the window advanced its checkpoint past uncommitted lower-lwt rows — those rows were invisible to checkpoint iteration forever (churn mode: 21/23 chunks of an updated file never reached the browser). | `rx_storage_helper.rs::DatabaseWrappedStorageInstance::bulk_write` | `checkpoint_iteration_never_skips_docs_under_concurrent_writers` |
| **Master pulls are authoritative in the browser LWW gate.** Master rows arrive without `_meta.lwt` (keep_meta=false), so their lwt falls back to the app-level `updated_at_ms` field — that heuristic must never veto a replication write. Only an unsynced LOCAL write (no `ctoxReplicationOrigin` marker) with a newer lwt may win. Accepted master rows keep the stored lwt monotonic. | Any master change whose payload timestamp did not advance was silently dropped while the pull checkpoint advanced past it — permanent divergence per document. | `storage-indexeddb.mjs::shouldAcceptDocumentWrite` / `bulkWrite` | `replication-lww-origin-smoke` |
| **Every browser-store write carrying master state passes `{ replicationOrigin }`** — replication pulls, query/file demand-loader materialisation, cache-eviction tombstones. | Unstamped demand-fetched docs counted as local writes: they vetoed later master pulls (above) **and** were push-eligible — cache-eviction tombstones (`_deleted: true`) of partial query windows could replay to the master as real deletions. | `query-demand-loader.mjs`, `file-demand-loader.mjs`, wired in `replication-webrtc.mjs::enableDemandLoading` | `replication-lww-origin-smoke` (§4) |
| **Local-push changed-since reads use the pushable index for CTOX-origin exclusion.** Browser storage keeps `pushable=1` only for local/browser writes and indexes `[collection, pushable, lwt, id]`; CTOX-origin rows are skipped by index selection, not post-cursor filtering. | File-sharing or demand materialisation created many CTOX-origin rows; the next local push walked those rows just to discard them, causing a local-write scan multiplier while the daemon was otherwise idle. | `storage-indexeddb.mjs::getChangedDocumentsSince` / `collectionPushableLwtId` | `storage-index-smoke` |
| **The desktop-file index scan is change-detecting and self-healing.** A rescan of an unchanged file is a byte-level no-op (fingerprint match + chunk-set completeness check); content changes re-chunk; incomplete chunk sets are repaired. | Every 15 s scan pass minted a fresh timestamped generation per file and tombstoned the previous one — ~200 docs/scan of insert/tombstone churn that pull (batchSize 2 for chunks) could never catch up with. | `rxdb_peer.rs::upsert_desktop_file_with_parent` | `rescan_of_unchanged_workspace_is_a_no_op` |
| **Materialisation is sticky.** Once `ctox.file.materialize` made a file `available`, the scan keeps maintaining it eagerly — it must never demote it back to its size/extension lazy policy. | The next scan rewrote the doc to `lazy` with an empty generation id, stranding replicated chunks; the file viewer reverted to unreadable ~15 s after every materialise. | `rxdb_peer.rs` (policy upgrade before the fast path) | `materialized_large_file_survives_lazy_rescan` |
| **Desktop file bytes are demand-fetched, not background-pulled or query-fetched.** `desktop_file_chunks` remains the native chunk store and browser upload push surface, but file viewing reads bytes via `rxdb.file.fetch` on `desktop_files`; the native source loads the active generation by deterministic chunk ids. | Opening a file or granting file access started normal `desktop_file_chunks` replication or `rxdb.query.fetch` over the chunk collection; native file fetches then used a Mango JSON full scan over the chunk table. Both paths could drive sustained SCTP/SQLite CPU while the daemon had no queued work. | `sync.js::isDemandOnlyPullCollection`, `replication-webrtc.mjs::shouldAttachQueryDemandLoader`, `file-viewer/app.js`, `file-demand-loader.mjs`, `rxdb_peer.rs::stream_demand_file_chunks` | `chunk-query-demand-disabled-smoke`; `demand_file_source_streams_active_desktop_file_generation` |
| **The desktop-file idle scan must not re-check every chunk of a verified generation.** Newly written or once-verified eager file docs carry `chunk_count` and `generation_verified_at_ms`; unchanged rescans use that marker instead of rebuilding the expected chunk-id list every 15 s. | Materialised large files stayed sticky `available`, but the idle scan still checked every expected chunk id on every pass. Large files therefore created periodic CPU spikes even when no file changed. | `rxdb_peer.rs::desktop_file_generation_verified_by_metadata` / `mark_desktop_file_chunk_generation_verified` | `materialized_large_file_survives_lazy_rescan`; targeted `rxdb_peer.rs` tests |
| **Active-collection gating must never lose events permanently.** Three sub-rules: (a) a peer that has never reported an active set is fail-open (all relays delivered) until its first report; (b) applying a new active set pushes one resync master-change per re-activated collection (closes the send→apply transit window); (c) the browser runs one checkpoint pull per newly-activated collection on every registry change. | Relays for "inactive" collections are dropped and browser pulls are purely event-driven — each hole left a collection permanently stale (viewer-restart soak mode: the browser file doc stayed `lazy` forever while the native doc was `available`). | `connection_handler_rs.rs::is_collection_active_for_peer` / `apply_active_collections` (+ the resync push in its message loop); relay drop point in `index_mod.rs`; `replication-webrtc.mjs` registry subscription | gating tests in `connection_handler_rs.rs`; `active-collections-catchup-smoke` (browser); viewer-restart soak mode |
| **The multiplex room handshake carries per-collection checkpoints** (`collectionCheckpoints`, mirroring `collectionSchemas`; key absent for single-collection rooms). | Collections deriving their protocol from the room handshake advertised the REPRESENTATIVE collection's checkpoint epoch — wrong-collection checkpoint evidence after every native restart. | `index_mod.rs::collection_checkpoints_payload`; consumed by `replication-webrtc.mjs::remoteProtocolForCollection` | `handshake_payload_omits_collection_schemas_when_none` |
| **A terminal `completed` command ack without `task_id` is success.** Control commands (`ctox.file.materialize`, `ctox.module.*`, …) are executed directly and intentionally never get a queue-task projection. | The command bus waited 45 s for a task that never comes — every control command dispatched through it failed. | `command-bus.js::waitForAuthoritativeQueueProjection` | `command-bus-projection-smoke` |
| **The 410 data-plane gate has an explicit control-plane allowlist** (subscription auth, `sync/native-peer/restart`). Control routes carry no Business OS records; the peer-restart route additionally answers 403 unless `CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS` is set. This is NOT a precedent for HTTP data routes. | The blanket 410 also killed the peer-lifecycle hook the rollover soak mode uses. | `server.rs::is_business_os_control_plane_path` | rollover soak mode |

---

## 9. Build & release

`dist/ctox-rxdb-js.mjs` is **built** from `src/index.mjs` with a pinned
esbuild. The exact command (pinned in
`tests/bundle-reproducible-smoke.mjs`, which rebuilds and diffs against the
committed dist):

```sh
npx -y esbuild@0.28.0 src/apps/business-os/rxdb/src/index.mjs \
  --bundle --format=esm \
  --outfile=src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs \
  "--banner:js=// CTOX DB app-local bundle. Generated from src/apps/business-os/rxdb/src/index.mjs."
```

**Cache-buster discipline.** The bundle is imported with a `?v=` query in
exactly two places, which must always carry the **identical** value:

- `src/apps/business-os/shared/db.js` (`RXDB_BUNDLE_URL`)
- `src/apps/business-os/shared/sync.js` (fallback dynamic import)

App modules do **not** import the bundle directly — they receive the database
handle from the shell facade (`setBusinessOsDatabaseContext`). The matching
module's `businessOsDataSource.js` used to be a third importer; it moved to the
facade, so it carries no buster and is no longer checked by the guard.

A mismatch makes the browser load a **second copy of the bundle** — two
module graphs, two shared-room-peer registries, duplicate peers in the room.
After any `src/` change: rebuild dist with the command above **and** bump the
buster in both files (current value at the time of writing:
`20260627-current-main-guard-v1`).

`src/scripts/vendor-builds/build-ctox-rxdb-js.mjs` does **not** build
anything: it verifies the manifest identity (name/public name,
`package_manager: none`) and copies the committed bundle plus a provenance
JSON (sha256 of bundle, manifest, README, every src/test file) for release
evidence. CI runs the provenance/dependency-audit asserts
(`.github/workflows/ci.yml`).

---

## 10. Test map

### 10.1 Browser suite (`src/apps/business-os/rxdb/tests/`)

`run-all.mjs` is the canonical entry point: runs every `*-smoke.mjs` in its
own node process (tests mutate globals), prints a pass/fail table, exits
non-zero on failure. Its header states the policy: *a red test is a finding,
not noise — never delete or weaken a test to make the suite pass.*

| Test | One line |
|---|---|
| `active-collections-catchup-smoke` | **Regression:** a collection transitioning inactive→active triggers one catch-up pull through the real shared-peer registry wiring (§8.1 gating invariant). |
| `advanced-status-bridge-smoke` | V1.5 → `business-os-advanced-status-v1` envelope mapping. |
| `bundle-reproducible-smoke` | **Guard:** dist must be byte-reproducible from src with the pinned esbuild (skips loudly offline; CI enforces). |
| `command-bus-projection-smoke` | **Regression:** queue commands wait for the task projection; control commands' terminal `completed` ack without `task_id` is success; `failed` rejects. |
| `compression-roundtrip-smoke` | JS decoder reads inline and deflate-compressed chunks shaped like the Rust dispatcher's. |
| `contract-drift-smoke` | **Guard:** re-runs both contract generators; generated files must match the fixtures (side-effect free). |
| `correctness-reconnect-smoke` | Demand-loader correctness and window invalidation across reconnects. |
| `cross-process-file-fetch-smoke` | E2E `rxdb.file.fetch` against the real Rust wire daemon over stdio. |
| `cross-process-wire-smoke` | E2E `rxdb.query.fetch` against the Rust wire daemon; chunk decode + doc verification. |
| `data-plane-guard-smoke` | **Guard (ratchet):** WebRTC-only, package-manager-free, env-toggle-free data plane; new forbidden occurrences fail, allowlist changes require an architecture decision recorded here. |
| `demand-loader-smoke` | Window cache hit/miss, single remote fetch, dedup. |
| `demand-loading-transport-smoke` | `replicateWebRTC` builds the demand transport; request/chunk correlation. |
| `end-to-end-loop-smoke` | Full V1.5 demand-loading loop. |
| `eviction-scheduler-smoke` | Sidecar eviction over budget. |
| `feature-flag-handshake-smoke` | Query-fetch capability lights only with capability + flag. |
| `file-demand-loader-smoke` | File chunk fetch, resume, concurrent dedup. |
| `frame-chunking-smoke` | **Regression:** byte-correct (JSON-escaped) chunk budgeting vs the chars-vs-bytes channel-killer. |
| `mixed-mode-handshake-smoke` | V1.5 browser vs V1 server handshake compatibility. |
| `multi-tab-broker-smoke` | BroadcastChannel leader election (and absence of BroadcastChannel). |
| `no-package-manager-import-smoke` | Bundle imports with no package manager present. |
| `orphan-cleanup-smoke` | Aborted fetches leave no partial documents/windows behind. |
| `primary-store-eviction-smoke` | Eviction deletes documents from the primary store, not just sidecar metadata. |
| `projection-window-gc-smoke` | Stale projection windows are garbage-collected. |
| `query-api-smoke` | Query API surface. |
| `query-fetch-capability-smoke` | Capability negotiation surface. |
| `query-fingerprint-corpus-smoke` | JS fingerprints match the shared JS/Rust corpus byte-for-byte. |
| `quota-recovery-smoke` | Sidecar behaviour under quota pressure. |
| `replication-demand-race-smoke` | Concurrent `masterChangesSince` vs query-fetch does not corrupt state. |
| `replication-lww-origin-smoke` | **Regression:** master pulls are authoritative in the LWW gate; unsynced local writes survive; demand loaders stamp the replication origin (§8.1). |
| `replication-recovery-smoke` | **Regression:** push re-run flag, pull retry, validity-keyed checkpoint retention. |
| `rollback-drill-smoke` | V1.5 activation leaves the V1 primary data path byte-identical. |
| `rtc-critical-pool-smoke` | Phase-3 multiplex admission contract + shell-critical collection set. |
| `schema-hash-registry-smoke` | Browser registry equals the Rust schema-hash fixture. |
| `sidecar-storage-smoke` | Sidecar store semantics (dirty/recently-read protection). |
| `signaling-freshness-smoke` | **Regression:** token re-stamp per connect, `yourPeerId`-only renames, backoff reset on `joined`. |
| `status-projection-smoke` | V1.5 status projection. |
| `storage-index-smoke` | IndexedDB storage index internals. |
| `v1_5-status-smoke` | V1.5 status field surface. |

### 10.2 Rust crate tests

The crate is **not** part of a cargo workspace (§4.1), so root `cargo test`
does not cover it. Canonical invocation:

```sh
cargo test --manifest-path src/core/rxdb/Cargo.toml
```

This runs the unit tests embedded in the modules (chunk splitter, signaling
reconnect chaos test, wire-frame classification,
`checkpoint_iteration_never_skips_docs_under_concurrent_writers`, …) plus
the conformance suite under `src/core/rxdb/tests/`. The desktop-file index
invariants (§8.1) live in the main binary's tests
(`cargo test --bin ctox rxdb_peer`):
`rescan_of_unchanged_workspace_is_a_no_op`,
`materialized_large_file_survives_lazy_rescan`. The cross-process JS smokes additionally
need the release wire daemon:

```sh
(cd src/core/rxdb && CARGO_TARGET_DIR=<repo>/runtime/build/cargo-target \
   cargo build --release --example v15_wire_daemon)
```

(They skip loudly when it is missing.)

### 10.3 Soak

`.github/workflows/rxdb-soak.yml` — manual (`workflow_dispatch`) only.
Drives the workflow's default mode matrix — 31 modes covering startup,
command/ticket round-trips, workspace artifact stress/churn, large-file
materialisation and the file viewer, daemon/signaling/native-peer restarts,
tab freeze, network flap, and ten injected-error status modes — for N cycles
on ubuntu-22.04 with `SOAK_FAIL_ON_RETRY=1` (a mode that only passes on its
retry attempt fails the run; default inputs resolve it to `1`). First fully
green run: 2026-06-10 on `ad81aff5`
(`rxdb_soak_summary ok=true cycles=3 retries=0`).

Every mode also runs locally, which is the fastest way to iterate:

```sh
cargo build --bin ctox
CTOX_BIN=$PWD/runtime/build/cargo-target/debug/ctox \
  SMOKE_PAGE_PATH=/index.html \
  SMOKE_MODE=workspace-agent-artifacts-churn-rust-to-browser \
  node src/core/rxdb/tools/browser_rust_smoke.js
```

(Playwright must be importable; the harness also honours
`PLAYWRIGHT_MODULE_PATH`. The harness validates `SMOKE_MODE` against the
list at the top of `browser_rust_smoke.js` — 41 modes in total; the soak's
default matrix runs 31 of them, the rest are UI/clarification modes driven
by other CI entry points.)

### 10.4 Canonical commands after touching the data plane

Anything under `src/apps/business-os/rxdb/`, `src/apps/business-os/shared/sync.js`,
or `src/core/rxdb/src/plugins/replication_webrtc/` (and `rxdb_peer.rs`):

```sh
node src/apps/business-os/rxdb/tests/run-all.mjs
cargo test --manifest-path src/core/rxdb/Cargo.toml
cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml
```

If `src/` of the browser runtime changed: rebuild dist + bump the three
cache-busters first (§9), since most smokes import from `dist/`.

---

## 11. Agent guardrails

Ranked by how much damage the corresponding mistake has historically caused.
Each has shipped (or would ship) real production breakage.

1. **No HTTP data bridge or fallback. Ever.** The boundary in §2 is the
   architecture's hardest rule; an HTTP path for collections/commands/files
   quietly becomes the path of least resistance and re-creates the legacy
   scaffolding this design exists to remove. Enforced by
   `data-plane-guard-smoke`.
2. **Never patch `dist/` directly; never change `src/` without rebuild +
   buster bump.** Both directions of src↔dist drift shipped breakage: a
   dist-only fix is silently reverted by the next build; a src-only fix never
   reaches the browser; a buster mismatch loads two bundle copies and
   duplicates peers. Enforced by `bundle-reproducible-smoke`.
3. **Never change wire contracts on one side.** Frame sizes, capabilities and
   RPC names live in the fixtures; a one-sided edit desyncs the peers and the
   resulting failures masquerade as network flakiness. Change fixture →
   regenerate → both consumers. Enforced by `contract-drift-smoke`.
4. **Never add npm/bare/`node:` imports to the browser runtime.** The runtime
   is deliberately package-manager-free, plain browser ESM; one bare import
   breaks every no-install deployment path. Enforced by
   `no-package-manager-import-smoke` and the dependency audit.
5. **Never add process-env toggles for runtime behaviour.** CTOX runtime
   configuration lives in the SQLite runtime store
   (`runtime_env::env_or_config`); env toggles fork behaviour invisibly per
   process and have repeatedly produced unreproducible states (see
   root `AGENTS.md` operator guardrails).
6. **Never delete or weaken a red test to make the suite pass.** Several of
   the regression smokes exist precisely because their semantics were once
   "cleaned up" out of the code; a red test is a finding
   (`run-all.mjs` header).
7. **Never re-enable initiator behaviour on the native peer.** The passive
   responder (peer-list task registers nothing; responder built on offer in
   `handle_signal`) is load-bearing: a pre-registered passive
   PeerConnection lets the browser offer hit the fast path and never get an
   answer — silent never-connecting peers.
8. **Don't "simplify"** (a) the role-based master election — native must be
   master toward browsers, the hash election alone reshuffles roles across
   reconnects; (b) the error-classification order in `sync.js` — moving the
   transient-blip branch back into the generic path turns every Wi-Fi blip
   into stop/start churn across ~80 collections; (c) the single-drainer queue
   discipline + `DrainResetGuard`/`remove_peer` cleanup — the wedged-drainer
   state it prevents parked senders forever and stalled all peers; (d) the
   byte-budgeted chunk splitter — char-based slicing kills the DataChannel on
   non-ASCII content.
9. **Generated files and the schema-hash registry are generated —
   regenerate, don't edit.** Hand-edits drift the two sides; a drifted schema
   hash silently quiesces that collection on every peer with no error that
   names the cause. (`*-contract.generated.*`, `business_os_schema_hashes.json`
   ↔ `CTOX_BUSINESS_OS_SCHEMA_HASHES`, `schema-hash-registry-smoke`.)

---

## 12. History pointers

- `src/core/rxdb/PORTING.md` — the port lookup table (upstream RxDB 16.20.0
  pin, per-module status); the per-wave ledger lives in
  `src/core/rxdb/revisions/` (79 wave files at the time of writing).
- `docs/rxdb_on-demand-load.md` — the V1.5 query-demand-loading plan and wave
  progress (sidecar, demand loaders, multi-tab broker, status surface).
- `docs/rxdb-realtime-stream-transport-plan.md` — the original proposal for
  the constant real-time stream + app-transparent transport. Historical:
  its Phase 1 (16 KiB ceiling, backpressure), Phase 2 (active-collection
  priority) and Phase 3 (single multiplexed room peer) are implemented; read
  the code, not the plan, for current behaviour.
- Commit `52a1bf45` — "fix(rxdb webrtc): close remaining sync-stability gaps
  (token freeze, wedges, recovery, resync churn)": token freshness
  re-stamping on both sides, send-queue wedge fixes, checkpoint retention,
  native-peer supervision. The follow-up hardening work (regression guards
  in §10.1 — frame-chunking, replication-recovery, signaling-freshness,
  contract-drift, bundle-reproducible, data-plane-guard, `run-all.mjs` —
  plus the byte-correct browser chunk splitter and in-source guard comments)
  lands in the commits immediately after it.
- The 2026-06-10 soak campaign (§8.1) — nine real findings fixed while
  driving rxdb-soak to its first fully green run: `680698d3` (lwt stamping
  under `locked_run`), `fbe84a02` (scan change-detection),
  `aee838d7` (control-command acks), `008a530b` (sticky materialisation),
  `24a1bf6f` (active-collection gating catch-up), `d53e1010`
  (`collectionCheckpoints` + control-plane allowlist), `ad81aff5`
  (browser LWW gate + demand-loader origin stamps).
- Directory-local agent rules: `AGENTS.md` files summarise §11 for agents
  working in guarded trees; neighboring `CLAUDE.md` files are import shims for
  Claude Code.
