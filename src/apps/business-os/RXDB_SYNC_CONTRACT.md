# Business OS RxDB Sync Contract

This is the only supported Business OS data path.

## Browser Peer

- RxDB version: `16.20.0`
- Storage: `getRxStorageDexie()`
- Local database: IndexedDB
- Transport: RxDB WebRTC replication with `simple-peer`
- Topic per collection: `{sync_room}:{collection}`

The browser writes user actions and module commands into local RxDB collections.
It does not call HTTP command, pull, push, or knowledge data bridges.

## CTOX Peer

- Storage: `RxStorageSqlite` from `src/core/rxdb`
- Local database: `runtime/ctox.sqlite3`
- Transport: RxDB WebRTC replication with `webrtc-rs`
- Signaling: same signaling URLs as browser peers

Until the Rust peer implements the full RxDB storage and WebRTC replication
contract, `/api/business-os/sync/config` must report:

```json
{
  "transport": "webrtc",
  "http_bridge_available": false,
  "native_rxdb_peer_available": false
}
```

That state means "sync peer not ready", not "load data through HTTP".

## Collections

Collections are module-owned and registered from each module's `schema.js`.
The shell registers core CTOX collections first, then each module registers its
own collections before opening.

Commands are normal replicated documents in `business_commands`. The CTOX Rust
peer consumes pending command documents, validates them, writes authoritative
state, and republishes accepted records through RxDB replication.

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
- cached harness snapshots presented as live state
- fake progress bars, fake live timers, or inferred CTOX state-machine steps
