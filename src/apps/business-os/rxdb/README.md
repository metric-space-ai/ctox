# CTOX DB (`ctox-rxdb-js`)

`ctox-rxdb-js` is the package-manager-free Browser-side CTOX DB runtime for
CTOX Business OS. It is derived from RxDB concepts and historical fork work,
but it is not upstream RxDB and is not a drop-in replacement for the npm
`rxdb` package.

Business OS app code must treat this as **CTOX DB**:

- public runtime name: `CTOX DB`;
- package/runtime id: `ctox-rxdb-js`;
- API contract: `ctox-db-business-os-v1`;
- upstream compatibility: `not-upstream-rxdb`.

Apps must not import `rxdb` or `rxdb/plugins/...`. They receive database and
collection handles from the Business OS runtime and use the CTOX DB contract.

Design constraints:

- plain browser ESM, no install step, no lockfile, no vendored dependency tree;
- native `indexedDB` storage through `getCtoxIndexedDbStorage()`;
- native `RTCPeerConnection` and `WebSocket` signaling without third-party
  peer packages;
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
