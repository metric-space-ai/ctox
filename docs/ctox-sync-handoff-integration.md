# Native session handoff integration boundary

Source audit: CTOX `a1b5e04f90333ba18f78fcca270c81b373e534b8`,
Workjet `f0ad31f297b921d7f96c054abf118840d0939789` (2026-09-20).
This records the remaining production integration for issue #183. It is not
acceptance evidence or a replacement for the full portability requirement.

## Existing paths and their limits

| Path | Observed production boundary | Missing connection |
| --- | --- | --- |
| Workjet `apps/server/src/workjet/mailbox/WorkjetHandoffSnapshot.ts` | Builds a bounded message-tail/context brief; explicitly excludes events, attachments, checkpoints and provider payloads. | It cannot supply the portable session manifest or durable provider state. |
| Workjet `WorkjetMailboxDelivery.ts::acceptHandoff` | Dispatches `thread.create` with a new thread ID, host model settings, null branch and null worktree. | This is a new contextual conversation, not continuation of the captured provider session. |
| Workjet `apps/server/src/workjet/sync/WorkjetSyncIpc.ts::requestSyncAuthority` | Typed private IPC client; current calls are in its test file. | No production execution owner invokes checkpoint protection or takeover through it. |
| CTOX `src/core/sync/src/native_execution.rs::activate` | Starts the private authority listener and supervises authenticated peer-route discovery. | Does not capture, stream, restore or resume a checkpoint. |
| CTOX `src/core/sync/src/capture.rs::CheckpointStore::capture` | Captures Git plus caller-supplied history/provider artifacts; requires the caller to establish quiescence. | No production session owner supplies that boundary and those artifacts. |
| CTOX `src/core/business_os/workjet_transfer_git.rs` | CLI pack/apply helpers reconstruct a working copy. | They are not called by the native session handoff lifecycle and do not establish account/principal permission. |
| CTOX `src/core/sync/src/authority/handoff.rs` | Verifies signed gate results and discards old evidence during revalidation. | The production gate adapter now exists (see below); transfer invocations remain absent — no production consumer calls `SessionHandoffTransfer`. |
| CTOX `business_session_handoff_bindings` | Migration creates binding fields and indexes; a production reader now exists. | No production binding enrollment path writes this table yet; rows are the gate's only authority. |
| CTOX `src/core/business_os/session_handoff_gate.rs` | Production `SessionHandoffGate`: per call re-reads the active binding by digest, matches side/phase, job, session, scope, checkpoint, ownership generation, harness/model-route/account/model against the row, requires this instance's enrolled identity for the side, resolves the principal's current role and capability epoch from `business_users`, demands the exact `session_handoff` grant on the binding, then mints a 60s signed permit. | Not yet invoked by an authenticated checkpoint sender/receiver; no enrollment writer feeds the binding table in production. |

The older Workjet snapshot module records an August decision to transfer only
a context brief. That behavior must not be relabeled as satisfying the current
native session portability objective. Do not silently change the context-brief
feature into a privileged native transfer; introduce the native lifecycle with
its own explicit authorization and retire superseded execution paths only after
its acceptance evidence exists.

## Required implementation sequence

1. Bind the native session owner to the durable job and provider session. The
   owner must stop accepting turns, await exact-turn termination and journal
   flush, then capture the admitted workspace and provider state. A UI thread
   ID, successful interrupt request or arbitrary supplied directory is not
   evidence that this boundary was reached.
2. Resolve and enroll the handoff binding from native authorities: authenticated
   source/target principals and epochs, current peer membership, admitted
   repository/base/working copy, provider account/model entitlement and policy
   revision. Exact grant lookup is necessary but cannot replace these checks.
   Do not make caller-supplied binding rows or a reachable model route the
   authority for workspace/account access.
3. Implement the Business OS gate against that authority, then attach it to the
   actual authenticated checkpoint sender and receiver. Disclose/receive
   decisions precede protected bytes; each bounded chunk and publication step
   must be fenced against revocation and account/policy changes after awaits.
   A permit-returning helper with no payload consumer does not meet this step.
4. Verify target reconstruction and durable copies before protection/takeover.
   Preserve original provider session identity, validate the new ownership
   generation, obtain current execute permission and resume through the native
   provider adapter. An unsupported import must fail visibly, never create a
   fresh conversation as a fallback.
5. Wire desktop, web and mobile requests through their host to that same native
   lifecycle. Remove replaced execution/transfer paths only once the new path
   and migration/recovery behavior are verified across those surfaces.

## Absent transfer consumer: owner contract

Source evidence (2026-09-27, CTOX `40569ba9f`): `SessionHandoffGate`,
`SessionHandoffTransfer` and `native_session_handoff_gate` have no production
caller. `src/core/sync/src/host_runtime.rs`, `native.rs`, `native_execution.rs`,
`ipc.rs`, `capture.rs` and `checkpoint.rs` contain no handoff reference; the
only constructions are in `src/core/sync/tests/session_handoff.rs` and the
adapter's own unit tests. The gate adapter therefore ships without its
consumer rather than with a fabricated one.

The smallest owner contract that closes step 3:

1. An **authenticated checkpoint sender** (owner: the native session owner
   from step 1) constructs `SessionHandoffTransfer::begin(gate, request)`
   before the first protected manifest or blob byte and calls `revalidate()`
   at every bounded chunk boundary; a denial stops the transfer and forbids
   resume until a fresh `begin` succeeds.
2. An **authenticated receiver** performs the same begin/revalidate fencing
   with `SessionHandoffPhase::Receive` before ingesting any protected byte.
3. The **target executor** obtains a `Resume` decision through the same gate
   after reconstruction verification and before starting execution (step 4).
4. The gate instance comes from
   `business_os::native_session_handoff_gate(root)` so permits are minted by
   the enrolled instance identity from the CTOX secret store; no consumer may
   substitute a caller-supplied key or a permit-returning stub.
5. Binding rows are written only by a separately authorized enrollment path
   that resolves the fields listed in step 2; that writer is still
   unimplemented and remains part of this owner contract.

## Acceptance evidence

Run the actual host lifecycle across separate processes/hosts with populated
journals, staged/unstaged Git edits, attachments and provider state. Demonstrate
same-session provider continuation, old-owner fencing, independent durable
replicas and recovery after source loss. Inject revocation before the first byte,
between chunks, during awaits and before resume; verify no later disclosure or
execution. Exercise wrong principal/workspace/account, peer replacement,
disconnect/reconnect, partial transfer, disk failure and expired permits.

Keep source revisions, byte/receipt identities and negative outcomes in the
result. Library tests, generated-contract parity, placeholder signatures and a
new thread containing a summary cannot establish this production acceptance.
