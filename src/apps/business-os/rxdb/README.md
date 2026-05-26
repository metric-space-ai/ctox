# ctox-rxdb-js

`ctox-rxdb-js` is the package-manager-free Browser-side RxDB hard fork
for CTOX Business OS. It is intentionally parallel to the current
`js-fork/source` bundle while the runtime is migrated off the dependency-based
fork.

Design constraints:

- plain browser ESM, no install step, no lockfile, no vendored dependency tree;
- native `indexedDB` storage instead of Dexie;
- native `RTCPeerConnection` and `WebSocket` signaling instead of simple-peer;
- protocol/schema contracts shared with `rxdb-rs`;
- small, explicit surface that can be audited like `rxdb-rs`;
- no feature gates, paid-tier checks, trial limits, or runtime add-on unlocks.

Current surface:

- `src/schema.mjs`: canonical JSON and WebCrypto SHA-256 schema hashes;
- `src/storage-indexeddb.mjs`: minimal collection storage over IndexedDB;
- `src/webrtc-native.mjs`: native WebRTC data channel with CTOX signaling
  metadata;
- `src/index.mjs`: public Browser ESM entry;
- `dist/ctox-rxdb-js.mjs`: stable import target for Business OS experiments.

The first migration goal is not upstream RxDB API parity. The goal is a CTOX
data-plane library with the specific primitives Business OS needs to sync with
`rxdb-rs` over WebRTC.

`addRxPlugin()` only exists as a transition shim for existing Business OS
bootstrap code. It does not unlock functionality; CTOX-required behavior lives
directly in this library.
