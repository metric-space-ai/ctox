# CTOX Sync Engine (ctox-rxdb) — The Business OS Data Plane

This is the reference document for CTOX Sync Engine: the WebRTC-only replication layer
between the browser-side Business OS shell and the CTOX daemon. It is written
for engineers and coding agents, and every technical claim in it has been
verified against the cited source file. When this document and the code
disagree, the code wins — and this document should be fixed.

Two implementations, one contract:

| Side | Name | Location |
|---|---|---|
| Browser | `ctox-rxdb-js` (public name **CTOX Sync Engine**) | `src/apps/business-os/rxdb/` |
| Daemon | `rxdb-rs` (crate `ctox-rxdb`, lib name `rxdb`) | `src/core/rxdb/` + `src/core/business_os/rxdb_peer.rs` |

---

## 1. What CTOX Sync Engine is

CTOX Sync Engine is a CTOX-owned data-plane runtime *derived from* RxDB concepts. It is
**not upstream RxDB** and not a drop-in replacement for the npm `rxdb`
package (`src/apps/business-os/rxdb/README.md`).

The identity contract is pinned in `src/apps/business-os/rxdb/manifest.json`:

| Field | Value |
|---|---|
| `name` (runtime id) | `ctox-rxdb-js` |
| `public_name` | `CTOX Sync Engine` |
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
always uses one path — CTOX Sync Engine over WebRTC between browser IndexedDB and the
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

Workspace branding (`business_workspace_branding`) is treated as Business OS
collection data under the same boundary: update through the Business OS command
path, replicate through CTOX Sync Engine/WebRTC, never through HTTP.

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
| `rx-database.mjs` | `createRxDatabase` / collection surface of CTOX Sync Engine. |
| `schema.mjs` | Canonical JSON + WebCrypto SHA-256 schema hashes; schema-hash registry; protocol payload helpers. |
| `storage-indexeddb.mjs` | Minimal document storage over native IndexedDB (default db `ctox_business_os_js_v1`). |
| `replication-webrtc.mjs` | `replicateWebRTC`: shared room peer plus per-collection replication states (pull/push, checkpoints, master handler). |
| `webrtc-native.mjs` | Native `RTCPeerConnection`/`WebSocket` peer: signaling, framed transport, prioritised send queue, request/response RPC. |
| `active-collections.mjs` | Registry of "active" (foreground) collections, derived from real subscriptions and recent `.exec()` reads; feeds the `rxdb.activeCollections` control frame. |
| `presence.mjs` | Ephemeral presence registry (ctox-presence-v1): per-owner local "who is viewing/editing what" entries feed the `rxdb.presence.update` control frame; remote aggregates arrive via the `presence$` push. Never persisted, never authoritative. |
| `conflict-merge.mjs` | Opt-in field-merge conflict strategy (§8.2): three-way merge of base/local/master at top-level-field granularity for collections declaring `conflictStrategy: 'field-merge'`. |
| `hybrid-logical-clock.mjs` | Stable browser-device Hybrid Logical Clock used to make whole-document conflict ordering deterministic across devices. |
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

### 3.1a Collection readiness — "unsynced" is not "empty" (SYNC-A wave F)

A collection that has never completed a pull looks exactly like an empty one if
you only count documents. Modules rendered "no data yet" during catch-up because
the readiness signal existed in diagnostics but never reached the render path.

The engine persists `firstPullCompletedAtMs` in the retained checkpoint record
(`replication-webrtc.mjs`), so readiness survives reloads and participates in the
existing validity-key invalidation — a permission or generation change resets it
correctly. From it, `collectionReadinessState` derives four states, exposed on
the transport status:

| State | Meaning |
|---|---|
| `never-synced` | No marker and no pull running. |
| `catching-up` | First pull is in flight. |
| `live` | Marker present — including at zero documents. |
| `offline-pending` | No marker, circuit open or no peer. |

The shell exposes exactly one selector, `ctx.sync.collectionReadiness(name)`
plus a subscription, fed from the existing diagnostics pipeline. The kit ships
`.ctox-syncing` as a sibling of `.ctox-empty`, and the rule for modules is:
**not ready ⇒ loading shell; ready and empty ⇒ `.ctox-empty`**. Rows that
already exist always render regardless of readiness.

Explicit non-goal: readiness is a **render hint, never a mount blocker**. The OS
stays snappy; a module must not wait for sync to appear.

### 3.2 Shell integration

**`shared/db.js` — `createBusinessDb({ name })`.** Imports the bundle from
`../rxdb/dist/ctox-rxdb-js.mjs?v=<buster>` (timeout-guarded, with one
cache-busted retry), runs an IndexedDB preflight probe, then
`createRxDatabase` with `getCtoxIndexedDbStorage()`. If the primary IndexedDB
stays blocked, writes fail with typed `indexeddb_blocked`; the shell does not
acknowledge writes into an alternate database that has no deterministic merge
path. A recovery journal and persistent per-collection unsynced-write counts
make destructive recovery visibly conditional on preserving unique writes.
It returns a façade (`addCollections`,
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
- *Room recovery:* error classification drives a shared-room circuit breaker;
  ordinary command tracking never restarts collections. Retryable transport
  failures recover the multiplexed room once, while terminal schema/auth
  failures remain stopped until relevant state changes. A 30 s
  native-peer-open deadline still guards room bring-up.
- *Watchdog:* a 30 s `nativePeerOpenWatchdog` per bridge escalates to
  `onFatalPeerError` if no native DataChannel opens.
- *Bounded send pressure:* collection registration is paced at 500 ms and the
  complete initial pull/push catch-up is serialized room-wide. The room-wide
  chain is reused after reconnects, when every collection is already
  registered; otherwise a reconnect would fan out every `masterChangesSince`
  request at once. A peer that still reaches the hard budget is treated as
  wedged and recycled.
- *Idempotent framed retries:* completed transfer ACKs are cached on both
  peers. A repeated `resume` or chunk from the final window replays the final
  ACK without decoding or delivering the payload twice; losing the final ACK
  therefore cannot tear down the shared multiplexed peer.
- *Suspend/resume:* `suspendCollections`/`resumeCollections` park bridges as
  `paused` without tearing the runtime down.
- *Demand-cache budgets:* ordinary collections retain a 6 MiB LRU working
  set. `knowledge_tables` retains 16 MiB so one complete chunked logical table
  can be assembled without evicting chunk zero mid-read. The shared browser
  ceiling remains 512 MiB.
- *Checkpoint handshake evidence:* a peer only counts as protocol-ready when
  it advertises `ctox-peer-session-v1`, `ctox-checkpoint-epoch-v1`, and an
  `advertised` checkpoint with a non-empty `epoch`
  (`hasNativePeerProtocolEvidence`, `classifyCheckpointProtocolError`).

`shared/sync-contract.js` supplies `collectionTopic`, `batchSizeFor`
(chunk-ish collections get small batches), and the `nativeRxdbPeerReady` gate.

Runtime-installed Business OS app collections are dynamic but still part of
the CTOX Sync Engine data plane. The browser registers them from each module's
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
- **Runtime-installed schema migrations are native too** (implemented
  2026-08-05, `migrate_additive_native_rxdb_collection_versions`; between
  2026-08-04 and then this paragraph was marked NOT IMPLEMENTED — a schema bump
  silently created an empty new table). For collections declared by
  `installed-modules/` or `local-modules/`, the peer reads the JSON-only
  `migration_strategies` from the same `collections.schema.json` used by the
  browser. It executes the supported declarative operations, verifies every
  source envelope in the target version, and only then permits stale-table
  cleanup. A missing strategy is tolerated only when the old source table is
  absent or empty; persisted old rows make bring-up fail closed. The browser
  half deliberately does NOT run `migrationStrategies` (`addCollections`
  ignores the field); the browser copy is treated as a replica — the
  invalidation path is tracked in `docs/ctox-migrations-plan-2026-08-05.md`.
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
file index, channel state, users, runtime settings, workspace branding, module
catalog, ticket state, knowledge tables, business-record projections) that
write core daemon state into the RxDB store for replication.

Workspace corporate design lives in the singleton
`business_workspace_branding/workspace-branding` document. Admin updates go
through `ctox.business_os.branding.update`; the native store validates allowed
semantic tokens and projects the result over CTOX Sync Engine/WebRTC.

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
  a browser-only token = first 32 chars of base64url(SHA-256(room password))
  with `token_iat`/`token_exp` (24 h TTL).
- **Signaling URL, native:** `rxdb_peer.rs::signaling_url_with_native_metadata`
  uses the stable `ctox-core-{instance hash}` identity,
  `role=ctox_instance`, and a separate random credential persisted only in
  the CTOX secret store. The browser never receives that native credential.
- **Cryptographic role binding:** both peers send
  `auth_version=ctox-role-bound-v1` plus SHA-256 commitments for the browser
  and native credentials. The signaling service derives one room namespace
  from both commitments and verifies the presented token against the claimed
  role. Re-labeling a browser connection as `ctox_instance` therefore fails;
  changing a commitment moves the peer into another namespace. Missing or
  invalid bindings fail closed. Canonical `wss://signaling.ctox.dev` URLs are
  migrated to the strict role-bound `/v2` endpoint; the worker root remains a
  temporary compatibility path for pre-v2 clients.
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
- When `native_peer_id` is configured, the browser accepts only a descriptor
  matching that stable identity. It does not fall back to another peer merely
  because that peer self-declares `role=ctox_instance`; supervised native
  process restarts keep the stable identity while `peer_session_id` still
  rotates for checkpoint and lifecycle invalidation.

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

**Error codes are a contract, and `retryable` is part of it** (SYNC-A-B2).
The JS side keys its retry/fallback behaviour on these codes, so a wrong code
is a behaviour bug, not a cosmetic one. Both handlers classify centrally —
`query_fetch_error_code_for_rx_error` and its file-side counterpart — rather
than mapping errors ad hoc at each `map_err`:

| Code | retryable | Meaning |
|---|---|---|
| `FILE_NOT_FOUND` | no | The file genuinely does not exist. |
| `FILE_SOURCE_ERROR` | **yes** | Transient source failure (I/O, DB, lock). Previously collapsed into `FILE_NOT_FOUND`, which made disk blips indistinguishable from a missing file and therefore never retried. |
| `UNAUTHORIZED` | no | Peer is not authorized for the collection/file. |
| `STREAM_LIMIT_EXCEEDED` | yes | Too many in-flight streams. |
| `RATE_LIMITED` | yes | Server-side throttle. |
| `REMOTE_TIMEOUT` | yes | Deadline hit. |
| `QUERY_NOT_SUPPORTED`, `SCHEMA_MISMATCH`, `FEATURE_DISABLED` | no | Terminal until config/schema changes. |

Adding a code is safe on this path: `routeFileError`
(`demand-loading-transport.mjs`) carries `code` and `retryable` through without
an allowlist, so an unknown code retries according to the server's flag rather
than being misclassified as fatal.

### 6.5 Presence (ctox-presence-v1)

Ephemeral "who is viewing/editing what" hints between browser peers, relayed
through the native peer. Presence is **advisory UX state only**: it is held in
memory on every side (no collection, no IndexedDB, no SQLite — idle stays
idle), and it must never gate an action; policy stays server-side.

- Browser → native: `rxdb.presence.update` control frame (like
  `rxdb.activeCollections`), params `[[entryObject, …]]`. Entries are opaque
  JSON objects, conventionally `{ collection, recordId, actorId, actorName,
  mode }`; the shell facade stamps the actor from the session. Capped at
  `maxEntriesPerPeer` (32) native-side.
- Native → browsers: the connection handler keeps the last report per peer and
  pushes each open peer the aggregate of every OTHER peer's live entries as a
  response frame with the reserved id `presence$` — on change, on peer close,
  and once after the TTL sweep (`ttlMs` 45 s). Entry-identical refreshes
  re-stamp the TTL clock without broadcasting.
- Idle discipline: the browser refresh timer (`refreshMs` 20 s) exists only
  while local entries exist; the native TTL sweep task exists only while any
  presence is stored. No presence ⇒ no timers, no frames.
- Capability-gated: both sides advertise `ctox-presence-v1`; the browser never
  sends the method to a peer that did not advertise it. Constants live in the
  protocol fixture (§7): `presenceRpc` + `optionalCapabilities.presence`.
- Shell surface: modules get `ctx.presence` (`set`/`clear`/`subscribe`) from
  the Business OS shell (`app.js::createModulePresenceFacade`), scoped per
  module and actor-stamped.

`desktop_file_chunks` is not a normal background-pull surface for browser file
reads. The browser disables pull and query-demand loading for chunk collections;
the file viewer opens `desktop_files`, then uses `rxdb.file.fetch` against the
`desktop_files` demand source. Native CTOX serves that source from
`desktop_file_chunks`, scoped to the active `desktop_files.content_generation_id`
and loaded by deterministic chunk ids, so opening a file does not scan or
replicate the full chunk store into IndexedDB. Browser-side chunk writes (for
uploads/attachments) may still use the chunk collection push path.

Runtime-installed modules can declare the same treatment for their own
collections (SYNC-32): in `collections.schema.json` a collection entry's
wrapper form may carry `"syncProfile": "eager" | "demand-only" |
"demand-chunks"` as a sibling of `schema` — exactly like `conflictStrategy`,
so the key is stripped before parsing/hashing and never shifts the advertised
schema hash. `demand-chunks` makes the native peer append a demand-file stream
source for the collection (self-served, after the unchanged built-in
`desktop_files`/`desktop_file_chunks`/`document_blob_chunks`/`spreadsheet_blob_chunks`
sources); the schema must then declare the owner key (`file_id` or `blob_id`),
`idx`, and base64 `data`, with chunk ids `{key}_{generation}_{idx}` — a
declaration missing those fields fails closed at collection registration
(`rxdb_peer.rs::module_dir_collection_entries`), not on the first
`rxdb.file.fetch`. `demand-only`/`eager` are parsed and validated natively for
the browser to consume later; browser-side derivation of the demand-only and
chunk-batch lists from the declaration is a separate step.

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

**Checkpoint contract.** `tests/fixtures/webrtc-checkpoint-contract.json`
(`ctox-checkpoint-contract-v1`) pins the checkpoint wire shape both sides must
interpret identically: the checkpoint status object's exact field list
(`source`, `state`, `collection`, `schemaHash`, `latestLwt`, `latestIdHash`,
`epoch`), the epoch derivation
(`epoch = sha256("{db}\n{collection}\n{schemaHash}\n{lwt}\n{id}")`), the
handshake keys it travels under (`collection.checkpoint`,
`collectionCheckpoints`, `storageGeneration` — string when set, JSON null when
empty), the `{id, lwt}` replication cursor, and the v1
(`epoch|sessionId|schemaHash`) / v2 (`storageGeneration|collection|schemaHash`)
validity-key formats — each with worked sha256 examples. This fixture is not
generator-backed; it is consumed directly by Rust tests in
`storage/sqlite/instance.rs` and `plugins/replication_webrtc/index_mod.rs` and
by `checkpoint-contract-smoke.mjs`, which drives the real
`checkpointValidityKeyFromProtocol` through the replication harness.

---

## 8. Failure & recovery semantics

| Failure | Mechanism | Where |
|---|---|---|
| Signaling socket drops (browser) | Self-reconnect with exponential backoff 1 s → 30 s; re-join re-broadcasts the peer list. Backoff resets on the `joined` broadcast, **not** on socket open — open-then-rejected sockets must keep backing off. | `webrtc-native.mjs::scheduleSignalingReconnect`, `handleSignalingMessage` |
| Signaling socket drops (native) | Supervisor task reconnects with 1 s → 30 s backoff using **fresh URLs from the `url_provider` failover list**: sticky on the last-working candidate, rotates to the next one only after a failed establish attempt (rotation never resets the backoff; that still happens only on `joined`). All configured signaling URLs participate — the list used to be cosmetic (only the first entry was ever tried). Covered by chaos tests in the same file (the extra test-only `TcpListener` binds raised the data-plane-guard ratchet for `signaling_client.rs` from 2 to 7 — an architecture-decision record for that allowlist change). | `signaling_client.rs`, `rxdb_peer.rs::signaling_url_provider` |
| Control-plane rejection | `ctoxError` frames are parsed and surfaced on both sides (the server closes the socket right after); otherwise a rejected join is indistinguishable from a blip and reconnects hammer silently. The browser shell additionally observes them via a WebSocket wrapper and treats them as fatal, non-retryable. | `signaling_client.rs`, `webrtc-native.mjs`, `sync.js::installSignalingErrorObserver` |
| Request vs disconnect race | `send_message_and_await_answer` subscribes to response **and** disconnect streams before sending and races them against a 60 s deadline; a peer dying mid-request fails the request instead of hanging the handshake/fork forever. Browser requests default to 15 s; a timed-out `ctoxProtocol`/`token` recycles the connection with `forceInitiator`. | `webrtc_helper.rs`, `webrtc-native.mjs::request` |
| Send-queue wedge | Exactly one drainer per peer queue (`draining` flag); `DrainResetGuard` re-opens the drain slot if the draining task is aborted mid-send; `remove_peer` drops the whole queue so parked senders fail fast instead of waiting on a drainer that no longer exists. | `connection_handler_rs.rs` |
| Handshake failure | `close_peer` force-closes that peer's transport so both sides observe a disconnect and rebuild cleanly — no half-dead channel-open-no-replication state. | `webrtc_types.rs::close_peer`, `index_mod.rs` |
| Reconnect resync churn | Pull/push checkpoints are persisted and retained on peer drop. Generation-v2 peers key them by persistent storage generation + collection + schema hash + current collection checkpoint epoch, so projection rewrites invalidate stale browser heads without forcing every daemon restart to resync. Mixed-version peers use checkpoint epoch + native peer sessionId + schema hash and therefore conservatively resync after a daemon restart. | `replication-webrtc.mjs::removePeer`, `checkpointValidityKeyFromProtocol` |
| Missed trailing writes / stale pulls | Push re-run flag (`pushAgainAfterCurrent`): a local write landing during an in-flight push triggers another pass. Failed pulls/pushes re-arm single retry timers using `retryTime` (min 1 s). | `replication-webrtc.mjs` |
| Browser-side repair | Error classification (§3.2) decides between recording a reconnect hint (blips, lifecycle events) and scheduling the unhealthy-collection sweep / full restart; an `active$` drop schedules a 750 ms restart. | `sync.js` |
| Native peer death | Supervised respawn with capped backoff; config re-read per attempt (room-password rotation applies); bring-up failure or stale heartbeat ⇒ fatal exit ⇒ respawn — never a zombie. | `rxdb_peer.rs` (§4.2) |
| SQLite contention while accepting durable background controls | Commands with durable identities and idempotent effects are returned to `pending_sync` with their payload and authorization receipt intact. The allowlist covers external-SQL sync/write, person research, and outbound source adapter generation/test/auth assistance; non-idempotent controls still fail closed. Accepted or non-terminal failed allowlisted rows remain restart-recoverable. | `store.rs::is_recoverable_background_control_command_type`, `rxdb_peer.rs::transient_business_command_retry_document` |

### 8.0 Waiting for quiescence (SYNC-A-C9b)

There is exactly **one** answer to "is the replication quiet?":
`replication_protocol::index_mod::await_rx_storage_replication_idle`, with
`await_rx_storage_replication_direction_idle` for callers that only care about
one direction. Before SYNC-A this existed four ways — a real subscription on
`active.up`, an acquire-and-immediately-drop of `stream_queue.up`, a fixed
double lock-drain in the protocol, and a second `for _ in 0..2` drain stacked on
top at the plugin layer. Two layers of the same approximation.

The primitive waits on retained signals, not on lock acquisition: activity is
published *before* throttling or queue acquisition and released by RAII when the
work actually ends, overlapping work is counted so no premature `false` escapes,
and the stream queues carry a retained idle flag that also covers waiting lock
acquisitions. Because the signals are retained, an already-idle state returns
immediately and a transition that happened before subscription is not missed.

Two properties are load-bearing — breaking either turns a wait into a hang:

- The primitive holds **no** stream or checkpoint lock while awaiting a signal.
  It takes the checkpoint lock only to drain and releases it before the final
  directional recheck; otherwise directional work could block on
  `set_checkpoint()` while the primitive waits on exactly that work.
- Lock ordering stays one-way. The `QueueOnly` path may wait for Up while
  holding Down; there is no inverse Up-to-Down wait in the protocol.

One caller keeps different semantics on purpose: the resolved-conflict path in
`persist_from_master` uses `QueueOnly`, because upstream RxDB awaits only the
current `streamQueue.up` head there. It must not wait for initial or unrelated
future upstream activity — full quiescence would be wrong, not merely slower.

### 8.1 Hard invariants (2026-06-10 soak campaign)

Each invariant below encodes a **real, reproduced data-loss or divergence
bug** found by driving the rxdb-soak matrix to its first fully green run
(`ok=true cycles=3 retries=0`, commit `ad81aff5`). Violating any of them
re-introduces silent data loss; every one is pinned by a regression test
that fails on the pre-fix code.

| Invariant | Bug it encodes (symptom) | Enforced in | Pinned by |
|---|---|---|---|
| **lwt stamping and storage commit are atomic** under `locked_run`. Never stamp `_meta.lwt` before taking the database lock. | Concurrent writers committed out of lwt order; a pull reading in the window advanced its checkpoint past uncommitted lower-lwt rows — those rows were invisible to checkpoint iteration forever (churn mode: 21/23 chunks of an updated file never reached the browser). | `rx_storage_helper.rs::DatabaseWrappedStorageInstance::bulk_write` | `checkpoint_iteration_never_skips_docs_under_concurrent_writers` |
| **Master pulls are authoritative in the browser LWW gate.** Master rows arrive without `_meta.lwt` (keep_meta=false), so their lwt falls back to the app-level `updated_at_ms` field — that heuristic must never veto a replication write. Only an unsynced LOCAL write (no `ctoxReplicationOrigin` marker) that outranks the master row may win: ordered by `_meta.ctoxHlc` when both sides carry one (SYNC-11 — the same ordering as push conflicts), by wall-clock lwt otherwise (mixed-version fallback). A local edit that loses the gate is journaled as `update_vs_update` unless the master row acknowledges it (own push round-trip). Accepted master rows keep the stored lwt monotonic. | Any master change whose payload timestamp did not advance was silently dropped while the pull checkpoint advanced past it — permanent divergence per document. Separately, pull ordering by wall-clock while push ordered by HLC made two concurrent LWW edits resolve interleaving-dependently, silently dropping the losing unsynced edit with no conflict record (SYNC-02 convergence mode). | `storage-indexeddb.mjs::shouldAcceptDocumentWrite` / `lwwOverwriteConflict` / `bulkWrite` | `replication-lww-origin-smoke`, `hlc-conflict-smoke`, SYNC-02 convergence E2E |
| **Every browser-store write carrying master state passes `{ replicationOrigin }`** — replication pulls, query/file demand-loader materialisation, cache-eviction tombstones. | Unstamped demand-fetched docs counted as local writes: they vetoed later master pulls (above) **and** were push-eligible — cache-eviction tombstones (`_deleted: true`) of partial query windows could replay to the master as real deletions. | `query-demand-loader.mjs`, `file-demand-loader.mjs`, wired in `replication-webrtc.mjs::enableDemandLoading` | `replication-lww-origin-smoke` (§4) |
| **Local-push changed-since reads use the pushable index for CTOX-origin exclusion.** Browser storage keeps `pushable=1` only for local/browser writes and indexes `[collection, pushable, lwt, id]`; CTOX-origin rows are skipped by index selection, not post-cursor filtering. | File-sharing or demand materialisation created many CTOX-origin rows; the next local push walked those rows just to discard them, causing a local-write scan multiplier while the daemon was otherwise idle. | `storage-indexeddb.mjs::getChangedDocumentsSince` / `collectionPushableLwtId` | `storage-index-smoke` |
| **The desktop-file index scan is change-detecting and self-healing.** A rescan of an unchanged file is a byte-level no-op (fingerprint match + chunk-set completeness check); content changes re-chunk; incomplete chunk sets are repaired. | Every 15 s scan pass minted a fresh timestamped generation per file and tombstoned the previous one — ~200 docs/scan of insert/tombstone churn that pull (batchSize 2 for chunks) could never catch up with. | `rxdb_peer.rs::upsert_desktop_file_with_parent` | `rescan_of_unchanged_workspace_is_a_no_op` |
| **Materialisation is sticky.** Once `ctox.file.materialize` made a file `available`, the scan keeps maintaining it eagerly — it must never demote it back to its size/extension lazy policy. | The next scan rewrote the doc to `lazy` with an empty generation id, stranding replicated chunks; the file viewer reverted to unreadable ~15 s after every materialise. | `rxdb_peer.rs` (policy upgrade before the fast path) | `materialized_large_file_survives_lazy_rescan` |
| **Desktop file bytes are demand-fetched, not background-pulled or query-fetched.** `desktop_file_chunks` remains the native chunk store and browser upload push surface, but file viewing reads bytes via `rxdb.file.fetch` on `desktop_files`; the native source loads the active generation by deterministic chunk ids. | Opening a file or granting file access started normal `desktop_file_chunks` replication or `rxdb.query.fetch` over the chunk collection; native file fetches then used a Mango JSON full scan over the chunk table. Both paths could drive sustained SCTP/SQLite CPU while the daemon had no queued work. | `sync.js::isDemandOnlyPullCollection`, `replication-webrtc.mjs::shouldAttachQueryDemandLoader`, `file-viewer/app.js`, `file-demand-loader.mjs`, `rxdb_peer.rs::stream_demand_file_chunks` | `chunk-query-demand-disabled-smoke`; `demand_file_source_streams_active_desktop_file_generation` |
| **The desktop-file idle scan must not re-check every chunk of a verified generation.** Newly written or once-verified eager file docs carry `chunk_count` and `generation_verified_at_ms`; unchanged rescans use that marker instead of rebuilding the expected chunk-id list every 15 s. | Materialised large files stayed sticky `available`, but the idle scan still checked every expected chunk id on every pass. Large files therefore created periodic CPU spikes even when no file changed. | `rxdb_peer.rs::desktop_file_generation_verified_by_metadata` / `mark_desktop_file_chunk_generation_verified` | `materialized_large_file_survives_lazy_rescan`; targeted `rxdb_peer.rs` tests |
| **Active-collection gating must never lose events permanently.** Three sub-rules: (a) a peer that has never reported an active set is fail-open (all relays delivered) until its first report; (b) applying a new active set pushes one resync master-change per re-activated collection (closes the send→apply transit window); (c) the browser runs one checkpoint pull per newly-activated collection on every registry change. | Relays for "inactive" collections are dropped and browser pulls are purely event-driven — each hole left a collection permanently stale (viewer-restart soak mode: the browser file doc stayed `lazy` forever while the native doc was `available`). | `connection_handler_rs.rs::is_collection_active_for_peer` / `apply_active_collections` (+ the resync push in its message loop); relay drop point in `index_mod.rs`; `replication-webrtc.mjs` registry subscription | gating tests in `connection_handler_rs.rs`; `active-collections-catchup-smoke` (browser); viewer-restart soak mode |
| **The multiplex room handshake carries per-collection checkpoints** (`collectionCheckpoints`, mirroring `collectionSchemas`; key absent for single-collection rooms). | Collections deriving their protocol from the room handshake advertised the REPRESENTATIVE collection's checkpoint epoch — wrong-collection checkpoint evidence after every native restart. | `index_mod.rs::collection_checkpoints_payload`; consumed by `replication-webrtc.mjs::remoteProtocolForCollection` | `handshake_payload_omits_collection_schemas_when_none` |
| **Native schema-version cleanup runs only after an additive migration copied and verified every source row.** Identity migrations are idempotent, preserve the newer destination row by `lastWriteTime`, and abort peer bring-up when any source id is absent or older in the destination. | Creating v1 metadata/table and crashing before the copy let the next startup classify the non-empty v0 table as stale and delete the only complete thread history. | `rxdb_peer.rs::migrate_additive_native_rxdb_collection_versions`, called after collection registration and before `repair_stale_rxdb_collection_schema_versions` | `additive_thread_schema_migration_copies_and_verifies_before_cleanup` |
| **Runtime app migrations are declared in JSON and enforced natively.** The browser side mirrors declarations (guarded for parity by `assert-declarative-migrations.mjs`) but execution is native-only. Every runtime collection with `version > 0` must provide every intermediate `migration_strategies.<collection>.<targetVersion>` entry. The native peer supports the same `set_from_first_truthy` and `set_boolean` operations as the browser plus identity migrations (`operations: []`). Missing strategies with persisted source rows abort before cleanup. | Browser-only `schema.js` functions left the native v0 store stranded or tempted operators into destructive same-version cleanup; schema changes made in place could also produce DB6 forever. | `shared/declarative-migrations.js`, `module_static_check.mjs`, `rxdb_peer.rs::migrate_additive_native_rxdb_collection_versions` | `runtime_installed_declarative_migration_is_discovered_and_copied`, `native_declarative_migration_matches_browser_operations`, `runtime_migration_without_strategy_retains_old_table_and_fails_closed` |
| **A terminal `completed` command ack without `task_id` is success.** Control commands (`ctox.file.materialize`, `ctox.module.*`, …) are executed directly and intentionally never get a queue-task projection. | The command bus waited 45 s for a task that never comes — every control command dispatched through it failed. | `command-bus.js::waitForAuthoritativeQueueProjection` | `command-bus-projection-smoke` |
| **The 410 data-plane gate has an explicit control-plane allowlist** (subscription auth, CTOX release check/apply, `sync/native-peer/restart`). Control routes carry no Business OS records; CTOX release actions are admin-gated and only read release metadata or launch the existing installer, and the peer-restart route additionally answers 403 unless `CTOX_BUSINESS_OS_ENABLE_SMOKE_CONTROLS` is set. This is NOT a precedent for HTTP data routes. | The blanket 410 also killed the peer-lifecycle hook the rollover soak mode uses. | `server.rs::is_business_os_control_plane_path` | rollover soak mode |

---

### 8.2 Conflict strategies (per collection)

Default semantics are whole-document LWW under the origin-aware gate above
(§8.1 "Master pulls are authoritative…"). Collections whose records several
people edit concurrently can opt into **field-merge** by declaring
`conflictStrategy: 'field-merge'` as a **sibling of `schema`** in the
collection definition (`schema.js`) — deliberately outside the schema object,
so schema hashes are unaffected. Everything below is browser-side only; the
native master needs no counterpart (the fork resolves conflicts in the RxDB
replication model).

Whole-document rows carry `_meta.ctoxHlc`. Browser writes advance a stable
device HLC; native rows preserve the field across `keep_meta=false` wire
normalization and receive a deterministic fallback stamp when absent. The
larger HLC wins in BOTH directions: on a push conflict
(`resolveWholeDocumentLwwConflicts`) and in the pull gate's local-veto
decision (`shouldAcceptDocumentWrite`, SYNC-11) — one ordering, so the
winner of two concurrent edits does not depend on relay-vs-push
interleaving. When either side lacks an HLC the pull gate falls back to
wall-clock lwt. An unsynced local edit that loses the gate whole-doc is
journaled into the conflict store as `update_vs_update` (the
`delete_vs_update` pattern generalized) unless the master row acknowledges
it — no silent drop. `business_commands` and `ctox_queue_tasks` remain
native-authoritative regardless of browser clock. This avoids
wall-clock-only LWW while preserving mixed-version behavior.

- The storage layer tracks a **merge base** per locally-edited row: the last
  master-confirmed doc before the local edit (`record.base`, never inside
  `doc`, never on the wire). Consecutive local writes keep the original base.
- A replication pull over an unsynced local row three-way merges
  (base/local/master) at top-level business-field granularity: a field only
  one side changed takes that side; a true same-field conflict keeps the
  LOCAL value (it pushes and round-trips). The merged doc is stored as a
  LOCAL (pushable) write — a deliberate, documented exception to the §8.1
  origin-stamp rule, because it still carries state the master has not seen —
  with the incoming master doc as the new base. Once no local-only change
  survives, the master row lands origin-stamped and the base clears.
- A `masterWrite` push conflict on a field-merge collection absorbs the
  master's conflict row the same way before the retry
  (`absorbMasterStateIntoConflictRows` in `replication-webrtc.mjs`) instead
  of force-overwriting whole-doc; the absorbed master row is written back as
  the EXPLICIT new merge base (`bulkWrite` `baseById`), so absorbed fields
  are not re-won as "local changes" on the next round. LWW collections use
  the HLC-ordered retry behavior above.
- Concurrent edits to arrays, ordered lists, rich text or other structured
  values do not receive an unsafe top-level merge. They fail with
  `structured_conflict_requires_resolution` for native/manual resolution.
- Observability: merge-enabled collections count `pullFieldMerges` /
  `pushConflictMerges` (`storageCollection.mergeStats`), surfaced per
  collection in the sync diagnostics alongside the checkpoint ages.
- Deletions stay whole-doc: a master tombstone wins outright; an unsynced
  local tombstone survives until it pushes.
- **Final deletes (opt-in).** By default a delete is whole-doc LWW, so a
  concurrent higher-HLC local update can win over — and resurrect — a
  tombstoned document. A collection that declares `deleteStrategy: 'final'`
  (an **independent sibling of `schema`**, orthogonal to `conflictStrategy` so
  a collection may combine `field-merge` updates with final deletes; outside
  the schema object, so schema hashes are unaffected and the native peer
  tolerates the wrapper unchanged) makes a tombstone on EITHER side beat a
  concurrent non-tombstone update regardless of HLC/lwt — on the pull gate
  (`shouldAcceptDocumentWrite`), the push-conflict path
  (`resolveWholeDocumentLwwConflicts`) and the field-merge path. Once deleted,
  a concurrent update can never resurrect the row; the losing update is still
  journaled as `delete_vs_update` (recoverable, not silently dropped).
  Two tombstones or two updates fall through to normal LWW/field-merge. Pinned
  by `final-delete-conflict-smoke` (which also proves default collections keep
  today's resurrect-on-higher-HLC behavior).
- Merge logic lives in `src/conflict-merge.mjs`
  (`threeWayMergeDocuments`), storage integration in
  `storage-indexeddb.mjs::resolveIncomingWrite`. Pinned by
  `field-merge-conflict-smoke`. First consumers: the customers module's
  record collections (`modules/customers/schema.js`).
- Runtime-installed modules declare the strategy in their `schema.js` exactly
  like static modules (the browser registers installed-module collections from
  `schema.js`). The native `collections.schema.json` parser additionally
  tolerates the same wrapper form per collection and unwraps it before
  parsing/hashing (`runtime_installed_module_collection_schemas`,
  rxdb_peer.rs), so generated apps may carry the declaration in both files
  without drifting the schema hash.

## 9. Recovery, multi-tab ownership and native sagas

Every pushable browser write is committed first to the separate IndexedDB
`${primaryName}__recovery_v2`. Its `batches`, `conflicts`, and `meta` stores are
not query caches and must never be evicted. Primary IndexedDB writes begin only
after the full batch, merge base, HLC, schema hash, instance/database identity,
and payload hash are durable. Startup registers collection replayers without
serially blocking shell schema registration. Each collection starts a
collection-scoped background replay; every mutating collection method still
awaits that initialization and therefore remains fail-closed before accepting
a new local write. Journal v3 adds the compound `stateCollection` index, so
registration and native acknowledgement inspect only pending batches for the
requested collection instead of scanning the entire WAL once per registered
collection. Batches that already carry `primaryCommittedAtMs` are not written
to the primary store a second time; they wait only for the native round-trip
acknowledgement. Replication-origin and demand-loading writes bypass this WAL;
the acknowledgement moves matching document/HLC entries to `master_acked`,
which is retained for 24 hours.

The database facade exposes:

- `db.recovery.getStatus()`, `export(passphrase)`,
  `previewImport(file, passphrase)`, `applyImport(previewId)`, and
  `retryPrimaryOpen()`;
- `db.conflicts.list()` and `resolve(id, resolution)` with `keep_local`,
  `keep_master`, or `restore_as_copy`.

Portable artifacts use `ctox.browser-recovery.v2` inside an authenticated
PBKDF2-SHA-256/AES-256-GCM envelope. Passwords are never persisted and v2 does
not permit instance remapping. Native tombstones remain authoritative, while a
delete-vs-update local version is retained in the conflict store. An unsynced
local update that loses the whole-doc LWW gate to a master row is retained
the same way as `update_vs_update` (SYNC-11) and additionally supports
`keep_local` resolution (re-applied as a fresh pushable write).

Exactly one tab owns the WebRTC line per database/room. Web Locks are primary;
a TTL'd BroadcastChannel lease with deterministic tie-break is the fallback.
Followers keep writing to their local primary/WAL and notify the leader of
dirty collections/ids. The leader broadcasts replicated invalidations back.
`freeze`/`pagehide` releases ownership; persistent checkpoints handle catch-up.

Peers advertising `ctox-checkpoint-generation-v2` validate retained
checkpoints with the persistent native storage generation, collection, schema
hash, and the collection's advertised checkpoint epoch. The epoch changes when
the native collection head changes, so a projection rewrite cannot reuse a
browser checkpoint that already advanced past newly projected rows. Process
session remains diagnostic only. Mixed-version peers keep the conservative v1
epoch/session behavior. Native time in the handshake anchors browser HLCs;
skew over five minutes is typed as
`clock_skew_detected`, and a strongly future HLC is persisted as a conflict
instead of winning LWW automatically.

Cross-collection browser commands do not describe transaction steps. The
native router selects a static saga definition whose forward and compensation
effects have durable idempotency keys in `ctox.sqlite3`. Lifecycle-v2 projects
the shadow fields `saga_id`, `saga_phase`, `saga_step`, `saga_total_steps`, and
`compensation_status`; modules treat a nonterminal saga as
`pending_consistency`. Failed compensation is `manual_intervention`, never
success. `ctox.module.set_visible` is the first registered two-step saga
(runtime visibility then RxDB catalog projection) and restores the original
visibility if projection fails.

### 9.1 Device-bound Workjet grants

`ctox business-os mobile-invite create` remains backward-compatible when no
device flags are supplied. A managed Workjet pairing supplies all three flags
together: `--device-pairing-id`, `--device-id`, and
`--proof-key-thumbprint` (the 43-character base64url RFC 7638 SHA-256
thumbprint of a P-256 public JWK). Partial tuples are rejected. The issuer JSON
echoes the adapter acknowledgement as `businessOsInstanceId`, `grantId`,
`deviceId`, `proofKeyThumbprint`, `expiresAt`, and `invite`; it never contains
the P-256 private key. Revocation accepts either the opaque invite id or,
additively, `--device-pairing-id`.

The tuple is persisted in nullable columns on `business_mobile_invites` and is
also HMAC-bound into the capability (`device_pairing_id`, `device_id`, and
`cnf.jkt`). Existing rows and ordinary Business OS capabilities remain
unbound. A bound capability is never accepted as an HTTP Bearer session.

The unified Workjet QR flow intentionally starts unbound because the desktop
does not know the phone's key before it scans the code. The first native
DataChannel handshake must complete the nonce-bound proof below before the
peer captures the token. CTOX then atomically binds that invite's unique user
to the P-256 thumbprint. The QR expiry limits this first use only. Reconnects
are authorized by the still-active Device-to-Instance edge, the actor's native
capability epoch, and a fresh proof; revoking either the edge or actor epoch
fails closed. The interactive token's normal expiry is never relaxed for HTTP.

On WebRTC, the native peer puts a fresh 32-byte base64url nonce in its outbound
`ctoxProtocol.peerSession.deviceProofNonce`. The browser calls exactly this
host boundary:

```js
Object.defineProperty(globalThis, 'ctoxWorkjetDeviceProofProvider', {
  value: async (nonce) => ({
    publicJwk: { kty: 'EC', crv: 'P-256', x, y },
    signature,
  }),
  writable: false,
  configurable: false,
  enumerable: false,
});
```

`signature` is base64url without padding of the 64-byte IEEE-P1363 P-256
ECDSA/SHA-256 signature over the UTF-8 bytes of `nonce`. Workjet must keep the
private key in its native key store; the callback exposes only the signature
and public JWK. The shell sanitizes those two public values into
`peerSession.deviceProof`. The callback is invoked once per native challenge,
including every reconnect, and its result is never cached. A rejection or
exception is treated like a missing proof. Missing provider, wrong key thumbprint, malformed
signature, missing/stale nonce, revoked tuple, or partial claims fail closed
before the native peer captures the capability or starts replication. An
unbound legacy token needs no proof and keeps the existing path. This is an
authentication extension inside the existing DataChannel handshake, not an
HTTP data bridge.

### 9.2 Workjet device control is an auxiliary WebRTC method

Device invitation and binding management use the existing authenticated
RxDB/WebRTC connection. The multiplexed native peer registers exactly one
auxiliary method, `ctox.workjet.device.v1`. Its single parameter is an exact
`WorkjetDeviceWebRtcRequestV1` object with one of these actions; unknown fields
and actions are rejected. Native results are validated and serialized through
the untagged `WorkjetDeviceWebRtcResponseV1` envelope before they cross the
DataChannel:

| action | fields | result |
|---|---|---|
| `invite.create` | `ttlSeconds?`, `displayName?` | the existing versioned Workjet/Business-OS invite envelope, returned only on the DataChannel |
| `invite.revoke` | `inviteId` | `{ revoked: true }` |
| `binding.list` | none | `ctox.workjet-device-bindings.v1` with non-secret device metadata and `inviteIdHash` |
| `binding.revoke` | `bindingId` | `{ revoked: true }` |

The shell exposes this method as
`globalThis.workjetBusinessOsDeviceControl(request)` and the native
mobile host forwards the same request through the bounded
`device.control`/`device.control.result` lifecycle bridge. Invite secrets are
never written to RxDB, HTTP, logs, reports, or the binding list. Cloudflare WSS
is used only by the normal WebRTC signaling exchange; it does not implement
this method and stores no device edge.

Each binding summary includes `inviteIdHash`, the lowercase hexadecimal
SHA-256 digest of the one-time `inviteId`. The inviting Workjet installation
hashes its in-memory invite ID locally and closes the QR only after
`binding.list` reports a paired edge with that exact digest. The raw invite ID
is never returned by `binding.list`; accepting an arbitrary new device edge or
comparing the device proof thumbprint to the invite ID is forbidden.

`invite.create` places only a 32-byte base64url bootstrap secret in the invite's
`session.capability_token`; CTOX stores only its SHA-256 hash. The secret is not
an HTTP bearer. Before first-use expiry it resolves to the pending actor only
inside the native WebRTC verifier. A successful nonce/P-256 proof atomically
adds the device id and proof-key thumbprint to the invite row. Later reconnects
require that exact active Device-to-Instance edge; revoke disables both the row
and actor epoch. This keeps the QR compact without an online reference service.

## 10. Build & release

`dist/ctox-rxdb-js.mjs` is **built** from `src/index.mjs` with a pinned
esbuild. The exact command (pinned in
`tests/bundle-reproducible-smoke.mjs`, which rebuilds and diffs against the
committed dist):

```sh
npx -y esbuild@0.28.0 src/apps/business-os/rxdb/src/index.mjs \
  --bundle --format=esm \
  --outfile=src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs \
  "--banner:js=// CTOX Sync Engine app-local bundle. Generated from src/apps/business-os/rxdb/src/index.mjs."
```

**Cache-buster discipline.** The only bundle URL lives in
`src/apps/business-os/shared/rxdb-runtime.js`. Both `shared/db.js` and
`shared/sync.js` import that loader with the **same literal APP_BUILD query
buster** (`./rxdb-runtime.js?v=<APP_BUILD>`). The data-plane guard checks both
values against each other and the shell build.

App modules do **not** import the bundle directly — they receive the database
handle from the shell facade (`setBusinessOsDatabaseContext`). The matching
module's `businessOsDataSource.js` used to be a third importer; it moved to the
facade, so it carries no buster and is no longer checked by the guard.

A different loader URL can create a second module instance; an unversioned
loader can keep an old bundle cached after a deployment. After any runtime
`src/` change, rebuild dist with the command above and bump the single bundle
buster in `rxdb-runtime.js`. Keep both loader import busters aligned with
APP_BUILD. The shared promise resets after import rejection, so a later call
can retry the same URL without creating a second bundle identity.

`src/scripts/vendor-builds/build-ctox-rxdb-js.mjs` does **not** build
anything: it verifies the manifest identity (name/public name,
`package_manager: none`) and copies the committed bundle plus a provenance
JSON (sha256 of bundle, manifest, README, every src/test file) for release
evidence. CI runs the provenance/dependency-audit asserts
(`.github/workflows/ci.yml`).

---


### Crew identity contracts (PR-2)

The existing channel migration seeds four stable members in `crew_members` and
adds `communication_routing_state.crew_member_id`. `crew_attempts` pins identity
and finalization accounting to an attempt; `crew_member_learnings` is the durable
learning source. These are additive Core changes, not browser version migrations.

`ctox_crew_members` (version 0) exposes `id`, `name`, `shape`, `color`, `archived`,
`state`, `active_task_id` and numeric `updated_at_ms` to every authenticated role.
Only Admin/Chef and Founder receive `soul`, `specialties` and `stats`.
`ctox_crew_learnings` (version 0) is readable only by Admin/Chef and Founder;
its fields are `id`, `member_id`, `text`, `kind`, `scope`, `evidence_run_id`,
`confirmed_by_owner`, `archived`, `created_at_ms`, `updated_at_ms`.
The pump is the sole producer, and missing source rows become tombstones.
Learnings are capped at 200 per member, evicting oldest unconfirmed rows first.
Required/index timestamps are numeric; malformed sources are skipped and logged
once, retaining the source for later repair.

All direct writes to both collections are denied, including explicit grants and
MCP writes. The same central policy prevents startup migration grants.
The native peer carries a per-actor field allowlist: masterChangesSince,
live changes and query-fetch all strip private member fields before serialization.
Queries cannot predicate, sort or select an index on a hidden field. MCP get,
query, search and related-record summaries use the same public field list.

`CrewManage` is `ctox.crew.manage`, with record scope ID equal to the full command
name. Admin/Chef may use `ctox.crew.member.create`, `ctox.crew.member.update`,
`ctox.crew.learning.confirm`, `ctox.crew.learning.update`,
`ctox.crew.learning.delete` and `ctox.crew.assign`. Founder may only confirm or
update learnings. User is denied; a grant cannot widen this permission.
Payload IDs are `member_id`, `learning_id` or `{task_id, member_id}` for assign.
Member creation takes `{name, shape, color, soul, specialties?}`; update takes
`{member_id, name?, soul?, specialties?, archived?}`. Learning update accepts
`{learning_id, text?, archived?}`. Assign requires an unleased pending/blocked task.
Controls reuse durable command receipts and write only whitelisted audit fields.

The PR-1 fields `ctox_queue_tasks.crew_member_id`, `ctox_runs.crew_member_id` /
`retrospective` and `ctox_harness_status.active_crew_member_id` now identify new
crew attempts. Historical attempts without identity retain their existing nulls.
The shared fixture is `src/core/rxdb/tests/fixtures/crew-identity.json`; native
field-policy tests and browser schema/permission tests consume it. Module JSON,
native schemas and both hash registries are regenerated before the pinned
esbuild 0.28.0 bundle build; the sole bundle URL remains in `shared/rxdb-runtime.js`.
## 11. Test map

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
| `checkpoint-age-diagnostics-smoke` | Per-collection checkpoint staleness: lwt recorded on transport activity (max across peers), `pull/pushCheckpointAgeMs` derived at snapshot time — no idle timers. |
| `checkpoint-contract-smoke` | **Guard:** checkpoint wire shape (status fields, epoch derivation, validity-key v1/v2 formats) matches the `webrtc-checkpoint-contract.json` fixture; drives the real validity-key code through the replication harness. |
| `command-bus-projection-smoke` | **Regression:** queue commands wait for the task projection; control commands' terminal `completed` ack without `task_id` is success; `failed` rejects. |
| `compression-roundtrip-smoke` | JS decoder reads inline and deflate-compressed chunks shaped like the Rust dispatcher's. |
| `contract-drift-smoke` | **Guard:** re-runs both contract generators; generated files must match the fixtures (side-effect free). |
| `correctness-reconnect-smoke` | Demand-loader correctness and window invalidation across reconnects. |
| `cross-process-file-fetch-smoke` | E2E `rxdb.file.fetch` against the real Rust wire daemon over stdio. |
| `cross-process-wire-smoke` | E2E `rxdb.query.fetch` against the Rust wire daemon; chunk decode + doc verification. |
| `data-plane-guard-smoke` | **Guard (ratchet):** WebRTC-only, package-manager-free, env-toggle-free data plane; new forbidden occurrences fail, allowlist changes require an architecture decision recorded here. |
| `demand-loader-smoke` | Window cache hit/miss, single remote fetch, dedup. |
| `demand-loading-transport-smoke` | `replicateWebRTC` builds the demand transport; request/chunk correlation. |
| `device-proof-smoke` | Native nonce-only Workjet callback boundary, public-only proof sanitizing, and no software-key fallback. |
| `end-to-end-loop-smoke` | Full V1.5 demand-loading loop. |
| `error-classification-corpus-smoke` | Shared corpus for the load-bearing error$ cascade order (control-plane → schema → IO → shutdown → lifecycle → blip → generic), incl. order-pin cases; the rxdb-rs twin keeps `ctox_rxdb_*` codes aligned with the generated contract. |
| `eviction-scheduler-smoke` | Sidecar eviction over budget. |
| `feature-flag-handshake-smoke` | Query-fetch capability lights only with capability + flag. |
| `field-merge-conflict-smoke` | Opt-in field-merge strategy (§8.2): three-way merge semantics, merge-base tracking, merged docs stored as local pushable writes, and untouched LWW pass-through for default collections. |
| `final-delete-conflict-smoke` | Opt-in `deleteStrategy: 'final'` (§8.2): a tombstone beats a concurrent higher-HLC update on all paths (no resurrection, loser journaled); default collections keep resurrect-on-higher-HLC LWW. |
| `sync-profile-registry-smoke` | Browser consumes the per-collection `syncProfile` declaration (name→profile registry populated in `addCollections`, read by `sync.js` demand classifiers) so runtime modules get demand-only/demand-chunks classification; built-in lists stay authoritative. |
| `file-demand-loader-smoke` | File chunk fetch, resume, concurrent dedup. |
| `frame-chunking-smoke` | **Regression:** byte-correct (JSON-escaped) chunk budgeting vs the chars-vs-bytes channel-killer. |
| `hlc-conflict-smoke` | Hybrid Logical Clock formatting, ordering and deterministic whole-document conflict decisions, incl. the HLC-ordered pull-gate veto both ways (SYNC-11). |
| `mixed-mode-handshake-smoke` | V1.5 browser vs V1 server handshake compatibility. |
| `multi-tab-broker-smoke` | BroadcastChannel leader election (and absence of BroadcastChannel). |
| `no-package-manager-import-smoke` | Bundle imports with no package manager present. |
| `orphan-cleanup-smoke` | Aborted fetches leave no partial documents/windows behind. |
| `presence-smoke` | Presence (ctox-presence-v1): registry union/debounce/refresh-only-while-nonempty, capability gate (no `rxdb.presence.update` toward pre-presence peers), `presence$` push routing, teardown clears remote hints. |
| `primary-store-eviction-smoke` | Eviction deletes documents from the primary store, not just sidecar metadata. |
| `projection-window-gc-smoke` | Stale projection windows are garbage-collected. |
| `query-api-smoke` | Query API surface. |
| `query-fetch-capability-smoke` | Capability negotiation surface. |
| `query-fingerprint-corpus-smoke` | JS fingerprints match the shared JS/Rust corpus byte-for-byte. |
| `quota-recovery-smoke` | Sidecar behaviour under quota pressure. |
| `replication-demand-race-smoke` | Concurrent `masterChangesSince` vs query-fetch does not corrupt state. |
| `replication-lww-origin-smoke` | **Regression:** master pulls are authoritative in the LWW gate; unsynced local writes survive (HLC-ordered veto, wall-clock fallback); losing local edits journal as `update_vs_update`; demand loaders stamp the replication origin (§8.1, SYNC-11). |
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

(They skip loudly when it is missing. CI builds the daemon and runs
`run-all.mjs --require-wire-daemon`, which turns a missing binary into a hard
failure — the cross-process smokes are the only proof that both sides agree on
real wire bytes.)

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
list at the top of `browser_rust_smoke.js` — 52 modes at the time of writing;
the soak's default matrix runs 34 of them, the rest are UI/clarification modes
driven by other CI entry points. The multi-writer mode
`concurrent-writers-convergence-browser-to-rust` drives TWO isolated browser
peers plus the native peer in one room and asserts LWW/field-merge/
delete-vs-update convergence across all three.)

### 10.4 Canonical commands after touching the data plane

Anything under `src/apps/business-os/rxdb/`, `src/apps/business-os/shared/sync.js`,
or `src/core/rxdb/src/plugins/replication_webrtc/` (and `rxdb_peer.rs`):

```sh
node src/apps/business-os/rxdb/tests/run-all.mjs
cargo test --manifest-path src/core/rxdb/Cargo.toml -- --test-threads=1
cargo clippy --manifest-path src/core/rxdb/Cargo.toml --all-targets
cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml
```

`cargo clippy` is part of the gate, not optional: the crate is clippy-clean
(zero warnings) as of SYNC-A-E2, so any warning you see is yours. Two lints are
allowed crate-wide with the reason written at the attribute in `lib.rs` —
`result_large_err` (boxing `RxError` would change every signature in a fork
whose error shape deliberately mirrors upstream) and, where it applies,
`type_complexity`. Every other `allow` in the crate is individually justified in
place; adding one without a written reason is a review finding.

`--test-threads=1` matters on a loaded machine: two SQLite tests
(`change_stream_emits_other_connection_sqlite_writes`,
`query_fallback_does_not_wait_for_writer_mutex`) time out under heavy parallel
load and pass in isolation. Check the load average before diagnosing them as a
regression.

If `src/` of the browser runtime changed: rebuild dist + bump the three
cache-busters first (§9), since most smokes import from `dist/`.

---

## 12. Agent guardrails

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


### Cockpit projections (PR-1)

The cockpit uses only the existing native-store → CTOX DB → WebRTC path. There
are no browser HTTP data endpoints. Source ledgers remain durable; retention
removes their Business OS/RxDB projections with tombstones, never core evidence.

| Collection | Source and stable identity | Retention |
| --- | --- | --- |
| `ctox_queue_tasks` | `communication_routing_state`; `id = message_key` | All nonterminal tasks plus newest 300 terminal tasks; typed runtime key `business_os.projection.queue_tasks_retention` accepts 1–100000. |
| `ctox_harness_events` | `ctox_harness_flow_events`; `id = event_id`, `task_id = message_key` | At most 200 events per task; terminal-task events expire after 24 h using the canonical terminal routing timestamp. |
| `ctox_harness_status` | Worker snapshots plus routing, persisted pause/capacity and working hours; `id = "harness"` | One singleton. |
| `ctox_runs` | `worker_attempt_finalizations`; `id = attempt_id` | Newest 500 completed attempts plus all attempts of nonterminal tasks. |

Queue schemas declare `execution_progress` (also on `business_commands`),
`lease_owner`, `lease_expires_at`, `lease_worker_id`, `first_pending_at`, `failure_class`,
`failure_attempt_count`, `retry_not_before`, `hold_reason`, `wait_entity_type`,
`wait_entity_id`, `priority_time_credit_hours`, `attempt`, and `crew_member_id`.
Cleared optional fields are written as explicit nulls. Routing timestamps retain
the source RFC3339 representation; projected event/run/status timestamps are
milliseconds. PR-1 always emits null crew identity and retrospective fields.

Events carry `task_id`, `command_id`, `attempt`, `kind`, `title`, `tool_type`,
`tool_name`, `call_id`, `success`, `usage {input,output,reasoning,total}`,
`runtime_seconds`, `step_position`, `created_at_ms`, `updated_at_ms`. Kinds are
`tool_started|tool_completed|thinking|plan_updated|token_usage|turn_completed|phase|crew_selected`.
Held-lease progress records carry eligibility without an extra routing read.
Unknown eligibility is omitted, so a missing routing row or unavailable state
cannot become a permanent denial; explicit false stays excluded after retry.
The pump resolves missing plan steps from the flow revision at emission time.
Raw tool arguments/results and
raw reasoning bodies are not copied. Indexes are `[task_id,created_at_ms]` and
`created_at_ms`.

The status singleton exposes `service_running`, `busy`, `paused`, `pause_reason`,
`worker_active_count`, `worker_phase`, `worker_capacity`, `pending_count`,
`leased_count`, `blocked_count`, `review_count`, `failed_recent_count` (24 h),
`pressure_active`, `pressure_threshold`, `work_hours {enabled,start,end,inside_window}`,
`active_task_ids`, `active_crew_member_id`, `last_error`, `boot_id`, `updated_at_ms`.
Queue counters count inbound queue work, matching the existing pressure guard.
Worker/queue hooks wake a bounded lossy writer independently of the stamp-gated
runtime-settings loop; a 60-second replay repairs missed delivery. These
collections have one dedicated producer, excluded from generic record replay.
Malformed `queue.pause` is treated as not paused, logged once until recovery and
reported in `last_error`; repairing the configuration clears that diagnostic.

Runs carry `task_id`, `command_id`, `work_id`, `crew_member_id`,
`status` (`succeeded|failed|timed_out|aborted`), `agent_outcome`, `started_at_ms`,
`finished_at_ms`, `metrics {model,provider,input_tokens,output_tokens,reasoning_tokens,cost_usd,tool_calls,thinking_turns,elapsed_ms}`,
`review {disposition,hold_reason}`, `error_text`, `resumable`, `retrospective`,
`updated_at_ms`. Billing joins actual `turn_id` links from attempt-tagged flow
records; missing prices/cost events remain null and later billing refreshes the
same run. Indexes are `[task_id,finished_at_ms]`, `finished_at_ms`, `crew_member_id`.
These indexes and both event indexes also exist on the native projection store.
Invalid optional start timestamps and the resulting unknown `elapsed_ms` remain null.
Invalid required/index timestamps cause the event/run document to be skipped, with
one diagnostic per root and collection. Source rows remain intact for replay after
repair; `created_at_ms`, `updated_at_ms` and run `finished_at_ms` are never emitted
as null.
Event-task and run queries use fixed-size keyset pages, including active runs.
There is no separate `attempt_id` alias in the run contract; `id` is the attempt ID.

Admin/Chef and Founder may read events, status and runs; User may not. These three
collections are server-authored: direct writes are denied for every role, even
with an explicit data grant. This decision is enforced centrally and by the
native peer write hook. Default and peer-start legacy grant migrations exclude
these cockpit collections entirely; they do not mint otherwise inert sync grants.
Queue controls go through `ctox.task.manage`: exact
commands `ctox.queue.release {task_id,priority?,note?}`, `.block {task_id,reason}`,
`.retry {task_id}`, `.capacity {workers}` (1–8), `.pause {paused,reason?}`, and
`.abort_turn {task_id}`. Release/block/retry/abort use task scope, capacity/pause
workspace scope. Controls preserve existing review/validation guards and record
the authenticated actor. Requested-audit payloads contain only typed `task_id`,
`priority`, `workers`, `paused`, `reason`, and `note`; audit text is capped at
1024 characters for task IDs, 16 for priority, and 1000 each for reason/note.
Unknown fields and wrongly typed values are excluded even when validation fails.
Each control persists its own terminal command receipt,
so duplicate command IDs replay the outcome without repeating the action. The
target task is not the control's `execution_task_id`.
Release/block/retry reject leased tasks. Release without `note` preserves the
existing status note; blocking a currently running slice is not supported.
Every linked Business OS aggregate must allow release: a single validating or
terminal aggregate blocks release/retry regardless of link order; retrying terminal
work requires a new command. Task IDs are trimmed consistently before policy and
execution. PR-3 must not infer queue-control authority from the browser helper's
`owned` fallback: these commands use native `BusinessOsScope::task(id, false, false)`
and do not infer ownership. A browser `owned=true` hint alone does not authorize them.
The current core schema additionally enforces one link per task; the release guard
also handles multiple links defensively without changing that unique constraint.
Abort currently returns
a failed command receipt with `result.status = "unsupported"` and `result.reason`.
The read/write restriction also applies to MCP:
module and record grants cannot bypass a cockpit collection denial.

`business_commands` (across all modules) and `ctox_queue_tasks` migrate 1→2;
`ctox_runs` migrates 0→1; events/status start at 0. Every existing browser DB
runs the corresponding no-op migration when next loaded. Packaged native upgrades use the existing
copy-and-verify declarative migration before stale-table cleanup. The module
schema, native contract/hash fixtures and browser schema are regenerated by the
standard generators. The browser DB and sync code share the sole bundle URL in
`shared/rxdb-runtime.js`; both import that loader with identical literal `APP_BUILD`
cache busters, guarded by the data-plane smoke test. A rejected import clears the
shared promise so it can retry the same canonical URL; there are no retry URL variants.

Consumers in PR-3/4/5 must query with selectors and indexed sort/limit, treat null
as unknown/cleared rather than zero, and use commands rather than editing a
projection. Chat messages additionally carry `kind`, `task_id`, `command_id` and
nullable `run_id`; `interim` is not proof of terminal success. Retention can remove
older views, so absence is not a claim that durable execution never occurred.

## 13. History pointers

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
