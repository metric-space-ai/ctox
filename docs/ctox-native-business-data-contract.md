# Native BusinessData integration contract

Status: implementation boundary agreed with the Workjet consumer on 2026-09-09.
This document specifies the remaining integration; it is not a declaration that
an IPC client, target resolver or resumable subscription is available.

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

A subscription emits an explicit initial snapshot, bounded pages, SnapshotEnd,
then ordered upsert/remove events. Every event carries session generation,
subscription ID and an opaque position. Partial snapshots remain visibly
incomplete. Membership/chat data from independent collections must not be
presented as one atomic snapshot without a shared source boundary.

The implementation must capture the snapshot and its change boundary together.
It must not read a checkpoint separately and claim it describes the streamed
query. In the current SQLite implementation,
`query_stream_on_dedicated_connection` opens its own read-only query cursor;
`replication_checkpoint_status` separately computes diagnostic state. Combining
those two calls does not establish a consistent snapshot plus resume boundary.

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
