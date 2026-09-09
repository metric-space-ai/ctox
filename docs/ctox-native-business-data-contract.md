# Native BusinessData integration contract

Status: implementation boundary agreed with the Workjet consumer on 2026-09-09.
This document specifies the remaining integration; it is not a declaration that
an IPC client, target resolver or resumable subscription is available.

## Generated host-consumer API

The logical request/response/event contract is now defined once in
`src/core/rxdb/tests/fixtures/ctox_business_data_contract.json`. Generate its Rust,
TypeScript and Effect schemas with the existing generator:

```sh
node src/core/sync/tools/generate-contracts.mjs --business-data
node src/core/sync/tools/generate-contracts.mjs --business-data --check
node src/core/sync/tools/generate-contracts.mjs --business-data --workjet-root /absolute/workjet/checkout
```

Rust exports `business_data_contract`; TypeScript lives in
`src/core/sync/contracts/ctox-business-data.generated.ts` and its schema sibling.
The Workjet destinations are `ctoxBusinessData.generated.ts` and
`ctoxBusinessData.schema.generated.ts` inside its existing contracts package.
Authority generation and its consumer pin are unchanged by this option.

`NativeBusinessDataRequest` covers saved-target open, status/close, scoped query,
watch/unwatch, submitCommand and observeCommand. `NativeBusinessDataResponse`
returns session state, bounded pages, subscription references or command state.
`NativeBusinessDataEvent` binds every subscription event to a native handle,
generation and sequence, with SnapshotStart/Page/End, recovery vs live deltas,
CaughtUp, Reset, Revoked and command outcomes. JSON business payloads remain
unknown at the TypeScript boundary and require their existing domain validation.

The native `business_data::decode_request` checks input shape and budgets. It
does not resolve a target, authenticate a session, authorize a selector, confirm
a command or mint a cursor. The generated types are an integration contract;
there is not yet an operational NativeBusinessDataClient or native data service.
Their binding must reuse the native private IPC lifecycle, retain bounded frame
assembly/backpressure and preserve authority framing limits. No new endpoint or
transport is introduced by the decoder. A completed page, native ready state or
SnapshotEnd must not be fabricated from this shape validation.

## Existing implementation and reuse boundary

- `src/core/sync/src/native.rs` owns the native transport session and now exposes
  `query_page`. `query_fetch_client.rs` in the RxDB crate consumes the existing
  `rxdb.query.fetch` acknowledgement/chunk/cancel protocol. It requires reciprocal
  peer admission, bounds a page to 200 documents and 2 MiB, and discards incomplete
  responses. This is the read primitive, not a logged-in Workjet data session.
- `src/core/sync/src/ipc.rs` and `local_host.rs` currently serve execution
  authority. Their private socket ownership, same-user check, framing deadlines
  and supervised teardown are reusable. Their existing 64 KiB authority frame
  limit cannot silently carry a 2 MiB BusinessData page. A business stream needs
  explicitly bounded frames and flow control; do not raise authority limits as
  an incidental change or open a second HTTP/TCP data service.
- `src/core/sync/tools/generate-contracts.mjs` generates Rust and TypeScript wire
  types from the RxDB fixtures. Extend this single generation mechanism when the
  BusinessData protocol is implemented. Do not hand-author competing Workjet DTOs.
- The native `validate_device_bound_peer_session` in
  `src/core/business_os/rxdb_peer.rs` validates incoming capabilities and device
  proofs. A tokenless peer can complete a least-privilege handshake; collection
  policy still restricts its reads. Consequently pool readiness alone is not a
  proof of the current user, the chosen target instance, or a usable data grant.
- The existing browser-live RPC in `rxdb_peer_browser.rs` carries Browser app
  input/frame exchanges. It is not a generic collection subscription API.

Workjet owns its host consumer and Web/Mobile presentation. The Sync core owns
native session authorization, remote data access and the local IPC contract.
The existing Project/Computer guest bridge remains a removal candidate until
native parity and real UI acceptance are proved. New Coding UI data access must
not depend permanently on `state.db` in a warm Business OS guest.

## Session and identity

The host resolves and authenticates the target instance and current user before
issuing a ready session. The renderer selects an existing authorized target; it
cannot supply an actor, bearer token, private key or trusted peer identity.
An opaque session handle and never-reused generation identify that binding.
Resolving, ready, disconnected and revoked are distinct observable states.

Each query, subscription and command status observation belongs to the handle and
generation. Switching instance/user closes old watches and invalidates in-flight
results before exposing the new session. Reconnection rechecks authorization.
Neither the signaling role nor the advertised room is sufficient target proof.
The production native target-proof and credential provisioning path is still
required; the native credential test fixture does not establish it.

## Query, snapshot and subscription

Requests specify a permitted collection, project/thread scope, supported ordering
and bounded page size. Native policy narrows the request and checks every read;
client-side filtering is presentation, never authorization. Initial consumers
need projects, working copies, computers, project chats, project workers, profile
bindings, public crew projections, selected threads/messages, sessions/transfers
and relevant command outcomes. They do not need whole-database renderer copies.

Subscription ordering is normative for the first consumer:

- The subscribed response is delivered before any event for that subscription.
- A fresh watch delivers snapshotStart, zero or more snapshotPage events,
  snapshotEnd, then caughtUp. Only caughtUp enters live state; snapshotEnd alone
  completes the snapshot payload and does not assert that recovery is complete.
- A valid resume delivers zero or more upsert/remove events with recovery=true,
  then caughtUp. New live upsert/remove events have recovery=false and follow it.
- A reset invalidates the prior view and cursor, then starts a fresh snapshot on
  the same subscription. Revoked and error are terminal for that subscription;
  reconnect requires a newly authorized subscription.
- Sequence starts at 1 and increases by exactly one across every event, including
  reset, for the lifetime of a subscription. Gaps or reordered events invalidate
  the local view and require recovery; a replacement subscription starts at 1.

Every event carries the session handle/generation and subscription ID. Snapshot
IDs identify one snapshot of one query in one collection. The first version does
not provide a shared source boundary across collections. Project chats and their
members therefore remain independently synchronized views; consumers must show
incomplete reconciliation instead of presenting them as one atomic snapshot.
Partial snapshots remain visibly incomplete. Cursors are opaque and bound to the
session authorization, collection and query; clients cannot compare cursor text
or infer a source revision from it.

The storage trait now exposes `query_snapshot_stream_into_blocking`. SQLite
implements it with a dedicated read transaction: the first read pins the existing
transactional collection change counter, then document batches use the same
transaction. Internal Start/Documents/End events let cancellation or read failure
finish without falsely completing a snapshot. The callback must run on a blocking
worker with bounded delivery and byte/time budgets supplied by the data service.
This primitive does not change the existing query-fetch wire or provide a watch.

The counter is source-local and is NOT a durable resume token. Collection
recreation, restored databases and a new source incarnation require a new epoch.
The service must bind the counter to authenticated source/schema/query identity
and still implement change retention, delivery and reset on unavailable history.
The existing trigger counter records changes including physical deletion and
writes with older timestamps, but it does not retain their document history.

The older `query_stream_on_dedicated_connection` and
`replication_checkpoint_status` remain separate reads. Combining those two calls
does not establish a consistent snapshot plus resume boundary. Unsupported
storage backends return no snapshot implementation; callers must reject the
operation instead of manufacturing a boundary from those separate reads.

The current query-fetch handler sets `authoritativeRevision` to the caller's
`query_fingerprint`. This value describes the query, not source data. It must
never become a snapshot revision or resume token. Until the storage/transport
boundary is implemented, `query_page` provides a completed bounded read only.

A valid resume delivers recovery deltas before new live events. An expired,
unavailable or unauthorized cursor produces an explicit reset and fresh
snapshot. Buffer overflow and missed events also reset; they never silently
skip ahead. Removed records must yield remove events, including records that
leave an authorized query because their scope or policy changed. Revocation
invalidates data visibility and ends the subscription.

Cancellation is owned by the existing native session lifecycle. A slow renderer
gets bounded backpressure or a visible reset/error, not an unbounded queue or
another polling supervisor. Mobile suspend persists only the necessary resume
reference and reauthenticates before resume; it cannot preserve a stale grant.

## Commands

Reuse the existing typed BusinessCommand and its durable command ID. The host
checks current actor, target scope and command policy before submission. The
initial consumer needs the existing project/chat/profile commands, then message
and execution operations through the same contract.

Pending, completed, failed and unknown outcome must remain distinct. Disconnect
or timeout preserves the command ID and reports uncertainty; it cannot imply
failure or generate a new business action on retry. Observe only relevant command
IDs. Existing domain receipts and native result recovery remain authoritative;
IPC must not create another journal of business truth.

## Integration and removal gates

1. Generate one Rust/TypeScript contract and exercise real native dispatch,
   malformed/oversized frames, cancellation and generation replacement.
2. Prove target authentication and current policy against the real Business OS
   host, including a ready but tokenless peer, wrong instance, revoked identity
   and changed user. Renderer-provided identities must be rejected.
3. Exercise writes and removals during snapshot paging, live delivery and resume;
   source restart, missed events, slow consumers and expired cursors must produce
   complete recovery or explicit reset. Test concurrent collection changes.
4. Wire the actual Workjet consumer. Test Desktop and Mobile project/group-chat
   UI, instance switching, suspend/resume and stable command outcomes. Measure
   click-to-chat presentation separately from authoritative command completion.
5. Run existing host budgets (warm command p50 < 300 ms; critical boot p95 < 5 s)
   and separately measure snapshot/resume under realistic data size and WAN.
6. Remove replaced guest data calls and their retry/status repair paths after
   parity; retain only genuine Business OS shell consumers. Add removal guards.

Passing native query tests or a presentation-model test alone satisfies none of
these complete integration gates. This work also does not certify SSH/QR worker
admission, harness checkpoint portability or automatic execution failover.
