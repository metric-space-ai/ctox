# Business OS RxDB Sync Contract

This is the only supported Business OS data path.

## Browser Peer

- RxDB version: `16.20.0`
- Storage: `getRxStorageDexie()`
- Local database: IndexedDB
- Transport: RxDB WebRTC replication with `simple-peer`
- Topic per collection: `{sync_room}:{collection}`

The browser writes user actions and module commands into local RxDB collections.
It does not call HTTP command, pull, push, status, session, module metadata, or
knowledge data bridges. The served app shell receives its launch-only session
and sync configuration through `window.CTOX_BUSINESS_OS_SESSION` and
`window.CTOX_BUSINESS_OS_CONFIG` injected into `index.html`; after that,
collection sync either starts WebRTC replication or stays explicitly
`local-only` until the native peer is ready. There is no browser-side
`/rxdb/pull` or `/rxdb/push` transport fallback.

## CTOX Peer

- Storage: `RxStorageSqlite` from `src/core/rxdb`
- Local database: `runtime/ctox.sqlite3`
- Transport: RxDB WebRTC replication with `webrtc-rs`
- Signaling: same signaling URLs as browser peers

The Rust port is complete. The CTOX daemon starts the native peer with
`RxStorageSqlite` and registers the Business OS collections before the browser
can replicate over WebRTC.

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

Commands are normal replicated documents in `business_commands`. Browser
dispatch writes a `pending_sync` document locally and returns immediately; it
does not POST the command to CTOX. The CTOX Rust peer consumes pending command
documents from `runtime/ctox.sqlite3`, validates them through the existing
Business OS command acceptance path, writes authoritative status/task fields
back into `business_commands`, and publishes the created queue task through
`ctox_queue_tasks`.

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

## CTOX Files

Files created or managed by the CTOX core agent loop must be written into the
native RxDB SQLite store, not sent through a separate HTTP file API.

- File index entries replicate through `desktop_files`.
- File payloads replicate through `desktop_file_chunks`.
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

## No Fallback Data

Business OS must not synthesize queue state, runtime state, module data, token
metrics, or flow progress when the real replicated records are missing.

Allowed local-only behavior:

- empty views while no replicated records exist
- explicit "native RxDB peer not available" status while the Rust peer is not
  complete
- static app shell and module manifests served by CTOX after login

Disallowed behavior:

- HTTP collection pulls as a replacement for RxDB replication
- HTTP command posts as a replacement for replicated command documents
- HTTP runtime flow or status snapshots presented as live CTOX state
- cached harness snapshots presented as live state
- fake progress bars, fake live timers, or inferred CTOX state-machine steps
