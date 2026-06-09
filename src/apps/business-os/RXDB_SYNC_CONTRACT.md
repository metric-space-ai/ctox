# Business OS CTOX DB / RxDB Sync Contract

This is the only supported Business OS data path.

## Naming and Compatibility

Business OS uses **CTOX DB** for its browser storage and sync runtime. The
runtime id is `ctox-rxdb-js` and the native peer is `rxdb-rs`, but this is not
upstream RxDB and it is not a drop-in replacement for the npm `rxdb` package.

The supported app-facing contract is:

- public runtime: `CTOX DB`
- runtime id: `ctox-rxdb-js`
- API contract: `ctox-db-business-os-v1`
- compatibility marker: `not-upstream-rxdb`

Business OS apps must use the database and collection handles supplied by the
Business OS shell. They must not import `rxdb` or `rxdb/plugins/...`, and they
must not rely on arbitrary upstream RxDB plugins, premium gates, or package
manager behavior.

## Browser Peer

- Runtime: CTOX DB (`ctox-rxdb-js`)
- Upstream provenance: historical RxDB `16.20.0` fork baseline, not a
  compatibility promise
- Storage: CTOX native IndexedDB storage via `getCtoxIndexedDbStorage()`
- Local database: IndexedDB
- Transport: CTOX DB WebRTC replication over native browser WebRTC APIs
- Topic per collection: `{sync_room}:{collection}`
- Room derivation: `ctox-business-os:{instance_id}:{sha256(room_password).base64url[0..22]}`
- Protocol marker: `ctox-rxdb-protocol-v1`
- Browser signaling metadata: `client=ctox-business-os-browser`, `role=browser`,
  `instance_id`, `protocol=ctox-rxdb-protocol-v1`, and repeated `cap` values
  including `ctox-rxdb-browser-v1` and `ctox-file-chunks-v1`.
- Native signaling metadata: `client=ctox-business-os-native`,
  `role=ctox_instance`, `instance_id`, `protocol=ctox-rxdb-protocol-v1`, a
  short-lived room-password-derived token, and repeated `cap` values including
  `ctox-rxdb-native-v1` and `ctox-file-chunks-v1`.
- Control-plane-capable browser and native peers both include the same
  room-password-derived token plus bounded `token_iat`/`token_exp` values. The
  Signaling server rejects missing or mismatched `ctox-rxdb-protocol-v1`
  metadata before peers can join Business OS rooms.
- ICE configuration: optional `ice_servers` / `iceServers` from the launch
  config, passed into the native WebRTC peer setup

The browser writes user actions and module commands into local RxDB collections.
It does not call HTTP command, pull, push, status, session, module metadata, or
knowledge data bridges. In local CTOX-hosted development the app shell can still
receive its launch-only session and sync configuration through
`window.CTOX_BUSINESS_OS_SESSION` and `window.CTOX_BUSINESS_OS_CONFIG` injected
into `index.html`. In web-deploy mode, the same WebRTC contract is read from a
paired browser config instead: URL parameter `ctox_config` (base64url JSON),
explicit `sync_room` + `signaling_url` URL parameters, `instance_id` +
`room_password` + `signaling_url`, or the persisted
`ctox.businessOs.pairingConfig` localStorage entry. Collection sync starts
WebRTC replication whenever the contract has `transport: "webrtc"`, a
resolved `sync_room`, and at least one signaling URL. The one-time
`native_rxdb_peer_available` status bit is diagnostic only and does not disable
WebRTC startup in the browser. There is no browser-side `/rxdb/pull` or
`/rxdb/push` transport fallback.

When a pairing config arrives through URL parameters, Business OS persists the
normalized config and immediately removes `ctox_config`, room password, and
signaling parameters from the address bar with `history.replaceState`. This
keeps the launch URL shareable only for the initial handoff and avoids leaving
the room password in browser history after the WebRTC/RxDB contract is loaded.

The supported deployment shapes share this same data plane:

- CTOX on a VPS with its own public IP/domain can host the static shell and
  inject the launch context directly.
- CTOX behind a managed `*.ctox.dev` subdomain uses that subdomain for hosting
  and launch context, but the collections still replicate through WebRTC.
- CTOX behind NAT or on a local machine does not need an inbound IP path. The
  browser receives a pairing config with signaling URL, instance id, and the
  room password; both peers derive the same non-guessable room and connect
  outbound to signaling.

If CTOX itself cannot host the browser shell, `ctox.dev` or the desktop app may
serve the same static Business OS assets and pass `ctox_config` into the URL.
That bootstrap is only app delivery and pairing context. Module catalog,
runtime status, commands, files, and channel data still come through
RxDB/WebRTC collections.

## CTOX Peer

- Storage: `RxStorageSqlite` from `src/core/rxdb`
- Local database: `runtime/ctox.sqlite3`
- Transport: RxDB WebRTC replication with `webrtc-rs`
- Signaling: same signaling URLs as browser peers
- ICE configuration: `CTOX_BUSINESS_OS_ICE_SERVERS` as JSON or comma-separated
  URLs, falling back to `stun:stun.l.google.com:19302`

The Rust port is complete. The CTOX daemon starts the native peer with
`RxStorageSqlite` and registers the Business OS collections independently of
the local Business OS webserver. `ctox business-os peer start` can run the same
peer as a foreground process for web-deploy setups where CTOX is not reachable
over IP and only connects outbound to signaling.

CTOX persists the Business OS WebRTC room password in the encrypted secret
store under `business-os/webrtc_room_password` unless
`CTOX_BUSINESS_OS_ROOM_PASSWORD` is explicitly supplied. The actual signaling
room includes only a hash-derived room id, so the raw password is a pairing
secret, not a room name.

While the peer is still starting, `/api/business-os/sync/config` reports:

```json
{
  "transport": "webrtc",
  "http_bridge_available": false,
  "native_rxdb_peer_available": false
}
```

Once the peer is running, `/api/business-os/sync/config` reports:

```json
{
  "transport": "webrtc",
  "http_bridge_available": false,
  "native_rxdb_peer_available": true
}
```

The old browser-side HTTP collection bridge has been removed from the sync
runtime. Any remaining HTTP endpoints are server/admin compatibility surfaces,
not the Business OS browser data plane.

## Collections

Collections are module-owned and registered from each module's `schema.js`.
The shell registers core CTOX collections first, then each module registers its
own collections before opening.

Commands are replicated transport envelopes in `business_commands`, not a
Business OS task system. Browser dispatch writes a `pending_sync` document
locally and must not report success until the CTOX Rust peer has consumed it,
created a real CTOX queue task, written authoritative status/task fields back
into `business_commands`, and published the created queue task through
`ctox_queue_tasks`. A local `pending_sync` document without a projected
`ctox_queue_tasks` row is not a task.

Business OS bug/feature reports are also command documents. Browser reporters
write `ctox.report.*` into `business_commands`; the Rust peer creates the CTOX
task, writes the canonical `business_module_reports` / `ctox_bug_reports`
records, and projects those collections back through RxDB SQLite.

Module source editing is a command/projection flow. The Source Editor writes
`ctox.source.load` / `ctox.source.save` commands; the Rust peer scans or writes
local module files, records snapshots where needed, and projects
`business_module_source_files` back through RxDB SQLite. The browser editor does
not call `/api/business-os/modules/source`.

Synchronous command-like browser reads are also projections, not HTTP calls.
For example, Knowledge `data select` runs as a `knowledge.command` document;
the Rust peer executes `knowledge::dispatch_capturing` and writes the JSON
response into `business_commands.result`, which then replicates back to the
browser.

Channel setup follows the same rule. Browser actions are `ctox.channel.*`
commands in `business_commands`; channel account lists and QR/pairing state are
read from replicated `communication_accounts` and `channel_pairing_state`
documents. The Rust peer projects those rows from canonical CTOX channel
storage into RxDB SQLite and writes tombstone documents when accounts disappear,
so Settings and Conversations do not call `/api/business-os/channels/*` as a
data path.

Business OS users are also a projection. Settings user mutations are
`ctox.business_os.user.upsert` commands, and the user list is read from the
replicated `business_users` collection. The Rust peer projects the canonical
store table into RxDB SQLite and the browser applies the same session visibility
rule locally.

Tickets follow the same command/projection rule. The Tickets app writes
`ctox.ticket.*` commands into `business_commands` for local ticket creation,
comments, transitions, approvals, execution, verification, writeback, and close
actions. The Rust peer executes those commands through the native CTOX ticket
capabilities and projects canonical ticket state back through the
`ctox_ticket_*` collections, including `ctox_ticket_items`,
`ctox_ticket_events`, `ctox_ticket_cases`, `ctox_ticket_self_work_items`,
`ctox_ticket_control_bundles`, `ctox_ticket_approvals`,
`ctox_ticket_verifications`, and `ctox_ticket_writebacks`. The browser does not
call a ticket HTTP API.

Runtime settings are projected the same way. Settings runtime/auth mutations are
`ctox.runtime_settings.save` commands, and the runtime status panel reads the
single `ctox_runtime_settings` document with id `runtime-settings`. The Rust
peer derives runtime, auth, service, and diagnostics fields from canonical CTOX
state and updates the document in RxDB SQLite periodically and immediately after
runtime save commands. The shell operational warning uses the same replicated
document; it does not poll `/api/business-os/status` for live CTOX state.

Business OS module metadata is also a projection. Settings admin reads module
manifests, template-store manifests, and governance metadata from the replicated
`business_module_catalog` document with id `module-catalog`; the shell uses the
same catalog for startup, background module refresh, and the template drawer.
The Rust peer refreshes that catalog from the local Business OS app tree and
canonical governance store periodically and after module commands.

## Collection Ownership And Write Model

Every replicated collection has exactly one of two write models. Which model a
collection uses is part of the module contract; mixing them up is how approval
and projection bugs happen.

| Write model | Who writes | Examples |
| --- | --- | --- |
| **CTOX-owned projection** — read-only in the browser; the only browser write is a command document in `business_commands`. The Rust peer validates the command, mutates canonical CTOX state, and republishes the projection. | Rust peer only | `ctox_ticket_*`, `ctox_queue_tasks`, `ctox_runtime_settings`, `business_module_catalog`, `business_module_reports`, `business_users`, `communication_accounts`, `channel_pairing_state`, `knowledge_tables` (pull-only) |
| **Module-owned records** — local-first browser data the module reads and writes directly through the shell DB handles (`ctx.db.<collection>`); replication carries the records to CTOX. | The owning module (and CTOX, as a peer) | `notes`, `documents`/`document_versions`, `spreadsheets`, `calendar_*`, `customer_*`, `accounting_*`, `planning_*`, `outbound_*`, `research_*` |

Rules that follow from this:

- A module may `insert`/`patch` only collections it owns. Anything CTOX must
  validate, execute, or audit goes through a `business_commands` command
  document, never through a direct write to a `ctox_*` projection.
- Modules access collections only through the shell context (`ctx.db`). The
  facade forwards collection names (`ctx.db.notes.find()` works); unwrapping
  `ctx.db.raw` is forbidden because raw handles go stale when the data plane
  recovers from schema drift.
- A module that lists another module's collections in `module.json` (for
  cross-links) does not own them and must treat them as read-only — and must
  accept that they only replicate once the owning module's schema has been
  registered on that client.

## CTOX Files

Files created or managed by the CTOX core agent loop must be written into the
native RxDB SQLite store, not sent through a separate HTTP file API.

- File index entries replicate through `desktop_files`.
- File payloads replicate through `desktop_file_chunks`.
- Each eager payload generation has a stable `generation_id`; the active
  generation is stored as `desktop_files.content_generation_id`. Browser
  viewers must read only that generation when present, so stale chunks from a
  previous write cannot mix with the current file contents. A generation is
  valid only when every chunk index from `0` through `total - 1` is present.
  CTOX keeps the active generation plus a bounded recent history. Older chunk
  generations are redacted and tombstoned through RxDB after the file row points
  at the new active generation, so delete events can replicate before later
  storage cleanup removes tombstones.
- CTOX-managed file rows keep the physical path in `local_path`/`path` for
  Rust-side materialization and expose the Business OS display path in
  `virtual_path`.
- `content_state` is part of the contract: `available` means chunks are already
  replicated, `lazy` means metadata exists and the viewer must dispatch
  `ctox.file.materialize`, `missing` marks a formerly scanned file that no
  longer exists locally, and `directory` marks virtual folders.
- Empty files still have one `desktop_file_chunks` row with an empty Base64
  payload, so viewers can distinguish a valid 0-byte file from missing content.
- Delivered workspace file artifacts from CTOX service jobs are upserted by the
  Rust RxDB writer into those collections, even when the WebRTC peer is running
  in another process. The peer then replicates the SQLite-backed records to the
  browser.
- After every completed CTOX service job with a `workspace_root`, the Rust
  writer also performs a bounded workspace-root index into the same collections.
  This makes files created by Codex tools and skills visible under
  `/CTOX/<workspace>/...` even when the prompt did not declare exact artifact
  paths up front. Small supported files are chunked eagerly; larger files stay
  lazy and are materialized on open.
- Document-like generated artifacts can additionally link to `documents`,
  `document_versions`, and `document_blob_chunks`.

The browser then sees CTOX-managed files through normal RxDB replication and
renders them as Business OS files.

## Regression Guard

`src/apps/business-os/scripts/assert-rxdb-only.mjs` is the automated guard for
this contract. It scans JavaScript, HTML, and JSON files in the app shell,
shared runtime, modules, desktop apps, Electron wrapper code, and template-store
app code for forbidden Browser-to-CTOX data paths, including `/api/business-os...`,
split-string variants such as `'/api/' + 'business-os'`,
`/api/business-os/status`, `/rxdb/pull`, direct command POST fallbacks, and
the removed native HTTP bridge helpers.

The main CI workflow runs the guard in the Linux CTOX check lane with Node 22.
Server endpoint definitions remain allowed as compatibility/admin surfaces; the
guard is scoped to browser-facing app code and the explicit native bridge
markers that must not return.

`src/apps/business-os/scripts/assert-module-conformance.mjs` is the companion
guard for the module contract itself: schema.js must declare every collection
the module lists in `module.json` (silent-sync-fail otherwise), index.js must
export `mount(ctx)` and must not unwrap `ctx.db.raw` or touch IndexedDB, and
module CSS must not write tokens on `:root`, redefine shell/base design tokens,
or `@import` remote stylesheets. Pre-existing violations are frozen in an
allowlist inside the script; do not add entries — remove them as modules are
migrated.

## No Fallback Data

Business OS must not synthesize queue state, runtime state, module data, token
metrics, or flow progress when the real replicated records are missing.

Allowed non-data behavior:

- empty views while no replicated records exist
- static app shell assets served by CTOX
- login/session bootstrap needed to inject `window.CTOX_BUSINESS_OS_SESSION`

Disallowed behavior:

- local replacement databases for Business OS data
- sync modes that keep Business OS data local instead of using WebRTC
- HTTP collection pulls as a replacement for RxDB replication
- HTTP command posts as a replacement for replicated command documents
- HTTP runtime flow or status snapshots presented as live CTOX state
- cached harness snapshots presented as live state
- fake progress bars, fake live timers, or inferred CTOX state-machine steps
