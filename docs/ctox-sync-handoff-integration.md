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

## Native guest acceleration selection

The native lifecycle owner can explicitly select KVM or single-threaded QEMU
TCG in `PreparedQemuGuest`. Both choices are available in production builds;
the enum is restricted to the native Business OS module and is not deserialized
from renderer or model requests. KVM never falls back automatically to TCG.
Both choices retain the same memory/vCPU bounds, paused process startup,
private transport, disk ownership and controller/quorum checks.

The approved second-host path uses GPU3 with KVM and GPU4 with explicitly
selected TCG. This option enables native configuration; it does not prove
guest restoration, target session continuation or measured desktop performance.


## Native operator guest enrollment

A configured foreground Sync host now retains its native guest registry and
attaches frame sources to its exact already-running native peer. Provisioning
uses its private Unix control socket, not HTTP, renderer-supplied host paths,
or a second Sync/Raft store.

The operator first runs `ctox sync configure-guests` with typed public JSON on
stdin: version 1, the canonical `computerId` of this host, and a nonempty
`requiredCapabilities` list matching the intended execution requirements.
These requirements are configuration, not newly issued capabilities. The
configuration is stored in the existing SQLite runtime store and takes effect
on the next host start. Absent configuration leaves guest enrollment disabled;
an unreadable or invalid configured store fails host startup.

`ctox sync guest-enroll <project-id> <thread-id> <worker-profile-id>` sends an
existing opaque Business OS session on stdin to that running host. The native
owner checks the local process UID and bounded frame, then resolves the
unrevoked/unexpired session, active principal, canonical project, chat, worker
profile and assigned computer in the same authoritative policy transaction.
Only the host creates the private import directory, guest and controller IDs.
The response contains identifiers, not paths, checkpoint payloads or execution
permission. Repeating the same current enrollment returns the retained
assignment, so a lost response does not create a second controller. Revoked or
replaced assignments fail closed and require reconciliation.

Registrations belong to this daemon lifetime. Restart does not resurrect a
live provider/process from a persisted claim; abandoned import directories
need explicit reconciliation. The provider turn producer, source/target
handoff binding enrollment, protected checkpoint transport and original-session
target resume are subsequent production connections under #183. Enrollment
alone does not start QEMU or establish two-host restoration.

## Native queue producer connection

The foreground service retains the registry created by its configured Sync
host and passes that registry to the ordinary queue turn producer, after the
existing external Crew executor has had its turn. An already-scoped Business
OS MCP chat task can select an enrolled guest from its canonical command
thread and the actual active Crew attempt. Queue metadata cannot select a guest,
profile, account or controller.

Selection checks the canonical command/task link, unchanged payload hash,
nonterminal command, current Crew lease and member, and the enrolled
project/chat/profile relationship. Group chats resolve through the active
profile's Crew member; another member's enrollment does not authorize it.
The same proof runs again on the held worker and policy transactions during
admission, binding installation and protected guest callbacks. Replaced command
scope, reassigned profile, expired/replaced lease, revoked controller and
changed policy fail closed.

The producer uses the existing durable account-bound native factory. It checks
the exact host-owned transport and command assignment before and after startup,
then installs the registry's native admission owner. It does not create or
clone another Sync peer. Native mode still requires its pinned direct ChatGPT
account and existing selected model; it cannot substitute a proxy, local model,
external executor or worker-profile route. Unenrolled tasks and canonical
external-executor commands retain their existing execution owners. Tasks that
have no existing signed Business OS MCP scope do not acquire one from guest
enrollment.

This connects the first-turn production producer only. It grants no checkpoint
disclosure, target receipt or target execution. An already-bound guest cannot
start a replacement provider session without lifecycle reconciliation.
Source/target handoff enrollment, complete artifact capture, authorized
checkpoint transport and continuation of the original session remain open.

## Native producer ownership before the capture transition

Source inspection at CTOX `5f2d52c362c628b0eea673c7eda8fe60d6e9be70`
(2026-10-06) identifies three separate lifetime boundaries:

- `service::run_foreground` retains the configured `sync_host::ServiceHost`
  for daemon lifetime. The host exposes its running `execution_authority`;
  no production caller currently connects that authority to
  `NativeGuestRegistry::new`.
- The regular chat producer in `execution::agent::turn_loop` calls
  `PersistentSession::start_with_business_os_mcp`. The separate
  `start_native_guest_with_business_os_mcp` entry point has no production
  caller. It requires an independently enrolled registry assignment and a
  retained native peer; a signed command token alone cannot supply those.
- `PersistentSession::run_turn_async` owns `NativeProviderTurnOwner` locally.
  On every return, its destructor revokes the live binding and removes the
  registry entry. The retained `NativeGuestExecution` observation handle
  does not extend that authority. A later turn is also expressly rejected
  until lifecycle reconciliation. Passing that handle to capture after the
  turn would therefore fail current-authority checks.

Checked shutdown now reaches the public persistent-session owner and its
review callers. It establishes checked teardown only. It neither transfers
the turn owner's authority to a capture owner nor independently resolves
the journal, provider artifacts, target account or handoff enrollment.

The production connection must retain the running host's authority through
an explicitly authorized native session lifecycle. That lifecycle must
enroll the actual project/chat/profile/controller and source/target account
bindings before execution. For capture it must retire mutable turn/command
rights, wait for checked termination and journal completion, and retain a
distinct source authority that can still validate the admitted session,
worker, account and policy while capturing and publishing. A cloned guest
handle, a finished worker row or a caller-supplied manifest cannot replace
that authority. Keep the current rejection of unreconciled subsequent
turns until this lifecycle is implemented; do not lengthen turn authority
merely to make capture pass.

These are source observations, not compiler, runtime or cross-host
acceptance results. The daemon's embedded pi sidecar remains the owner of
Business OS app coding turns; native session handoff must not redirect
`ctox.coding.turn` into the Codex guest producer.

## Native file-transfer publication composition

The existing transfer carrier now registers grant/provision services through the
guarded auxiliary dispatcher. Each queued reply retains the original accepted
peer and token. Every physical callback rereads the current encrypted capability
issuer, source identity and exact grant or routing credential tuple, then holds
current policy and any required file-generation projection transaction through
that callback. A changed principal, device, epoch, issuer, scope, content
generation or accepted connection cannot publish the prepared success.

The composed native framing implementation keeps main's pipelined cumulative
ACK windows. Capacity waits, each physical chunk, restart and resume retain their
own completion owner and publication guard; the existing transfer guard retires
all outstanding ACKs on termination. The owned GuardedChunkLease interface
remains the native frame-source boundary.

Registered-service regressions use real native stores and P256 admission with a
controlled zero-byte Pending sink. Separate production framing regressions cover
current pipelined delivery, second-window revocation/owner retirement and revoked
resume. These tests are unrun on this composition. They are not Linux/two-host or
full session-handoff acceptance. This file-transfer connection does not supply
the missing session enrollment, provider/artifact export or target-resume path.

## Recorder-owned journal capture

The admitted native producer retains its actual loaded CodexThread from the same
ThreadManager that starts the embedded client. Capture asks that Core Session's
actual RolloutRecorder for one opaque descriptor; neither a path from ThreadRead
nor a caller's session label can select the file. Retention refuses an
unmaterialized recorder and does not create a deferred journal.

The reader stays sealed until the writer flushes all preceding commands, closes
its command receiver and records the descriptor's final metadata. Failed writer
shutdown cannot seal it. After checked Core/client/runtime teardown, the native
capture owner rechecks current worker/account/policy/command authority and reads
bounded bytes from that retained descriptor. Metadata changes since shutdown or
during the read reject the candidate. The existing strict portable journal
validator binds those bytes and their SHA-256 to the actual producer thread ID;
malformed, truncated, unsupported or conflicting journals cannot return a capture
owner. Later reads repeat authority and journal checks.

This supplies actual journal input only. It does not establish provider resume,
capture every attachment or VM file, reconcile effects, enroll the handoff,
publish protected bytes or reconstruct a target. No checkpoint is published with
an empty or fabricated provider-state blob. The native factory still needs its
explicit production lifecycle caller.

Four added recorder regressions exercise real writer retention and path
replacement, post-shutdown mutation, failed writer shutdown and deferred-thread
refusal. Formatting and source checks are separate from compilation/runtime
evidence; these new tests have not yet been run in the shared composed verifier.

## Capture-only source ownership

The native producer now consumes its actual turn owner after a successful reply
and an exact thread/turn-scoped TurnComplete witness. A prematurely closed stream
with an existing reply cannot retire to capture authority. The persistent session
retains a distinct NativeProviderCaptureOwner; old live-provider lookups, command
emitters and previously admitted command witnesses reject the retired phase.
The original worker transaction and pinned account are reread by each capture
callback, and the registry requires the exact same retained producer record
before checking current project/profile/policy/controller authority. The source
also retains its original signed command token/context and revalidates them
around each callback, outside the held native stores to avoid reentry. These
checks do not replace the separate authority held through protected byte IO.

PersistentSession::quiesce_native_capture consumes that native source, requires
both real shutdown owners and a non-ambiguous turn, validates current authority,
performs checked client/runtime teardown, then validates current authority again.
No handle escapes failed or forced teardown. Dropping the capture owner revokes
the original provider record. Ordinary producers do not gain this authority, and
the subsequent-turn reconciliation refusal remains.

This implements the native source ownership transition and its explicit
quiescence boundary. It does not connect the native factory to service startup
or invoke CheckpointStore::capture. The returned handle is not fresh Raft
ownership, complete artifact/provider export, handoff enrollment, effects
reconciliation, protected-byte publication or target-resume certification.
Those production consumers and acceptance requirements remain open.

Four new regression sources cover retained mutable consumers and actual worker
transaction fencing; unbound/foreign/unpinned retirement; worker/account
revocation after retirement; and ordinary/ambiguous capture refusal through a
real embedded client/runtime. Their compiler and runtime validation remain
pending in the existing composed verifier.

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
   at every bounded chunk boundary. The actual transport must also retain
   current native authority and the exact accepted connection through each
   synchronous byte-publication poll, including retries after an await.
   A previously signed permit or a successful asynchronous revalidation is
   not that publication fence. A denial stops the transfer and forbids resume
   until a fresh `begin` succeeds.
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

## Current native admission authority

The production gate retains only its independently pinned public issuer.
Construction and every `authorize` call borrow the freshly decoded existing
Sync identity through the encrypted secret store's mutation fence. Authorization
then holds one nonblocking `BEGIN IMMEDIATE` transaction on the existing
Business OS store through binding, current user/epoch, exact grant and signing.
Missing, malformed, deleted, rotated or busy issuer authority denies; a rotated
key cannot silently replace a live gate's pinned issuer. Missing or blank policy
stores deny without schema initialization or migration.

These fences end when the synchronous admission decision returns. They do not
cover subsequent asynchronous checkpoint bytes, peer membership, current
provider-account entitlement or filesystem replacement outside native runtime
ownership. The production enrollment writer, quiescent capture owner and actual
sender/receiver/resume consumer remain required. Three source regressions use
the production constructor and decision path with fixture enrollment rows for
key deletion/rotation, competing issuer/policy writers and absent/uninitialized
stores; they are not real session-transfer acceptance.

## Atomic effect-checkpoint reconciliation

The source composition also requires a fresh, signed Disclose permit on
`CommitEffectCheckpoint`, bound to the exact request, job, copy digest/sequence
and current ownership. Admission checks its validity window before proposing
the entry; deterministic apply verifies its enrolled signer and binding.
The same entry still replaces the checkpoint and closes only its sole pending
effect. It cannot clear another pending effect or reuse an older sequence.

The unchanged-legacy-checkpoint upgrade is limited to a clean checkpoint and
ordinary `ProtectCheckpoint`. It cannot refresh a dirty checkpoint after
ordinary effect completion, or complete a live effect with unchanged bytes.
The production enrollment, held publication guard and actual sender/receiver
remain required; this reconciliation does not supply them or claim acceptance.

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
