# Native session handoff integration boundary

## Core import primitive

The trusted native execution owner can now call
`ThreadManager::resume_thread_from_native_checkpoint` with a strictly decoded
checkpoint, fresh target configuration/authentication and a private mutable
copy of the original journal. The imported Core retains the original thread
ID, compacted history and completed provider response chain. Import starts no
turn and grants no permissions; model sampling requires a subsequent explicit
submission. The manager rejects concurrent creation and cannot be reused for
another import or ordinary startup.

The actual Core regression captures a completed model turn, shuts down the
source, imports into a fresh target home/workspace and runs the next turn. Only
the model endpoint is mocked. This does not prove current native account,
policy, quorum ownership, effect reconciliation or independent-host acceptance.
The production target currently decodes the protected Core artifact before
workspace preparation and still returns `resumed:false`. Connecting authorized
execution admission to the prepared workspace remains the next #183 slice.

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
| CTOX `src/core/sync/src/capture.rs::CheckpointStore::capture` | Captures Git plus caller-supplied history/provider artifacts; requires the caller to establish quiescence. | The configured native queue owner now supplies its stopped Core journal/state and explicitly assigned Git working copy. The protected native copy path now transfers it to target storage; effect reconciliation and original-session resume remain open. |
| CTOX `src/core/business_os/workjet_transfer_git.rs` | CLI pack/apply helpers reconstruct a working copy. | They are not called by the native session handoff lifecycle and do not establish account/principal permission. |
| CTOX `src/core/sync/src/authority/handoff.rs` | Verifies signed gate results and discards old evidence during revalidation. | The production gate adapter now exists (see below); transfer invocations remain absent — no production consumer calls `SessionHandoffTransfer`. |
| CTOX `business_session_handoff_bindings` | Migration creates binding fields and indexes; a production reader now exists. | The local operator enrolls the actual native source capture and a source-signed offer against target-local policy; the guarded native copy command now transfers it; Core resume remains open. |
| CTOX `src/core/business_os/session_handoff_gate.rs` | Production `SessionHandoffGate`: per call re-reads the active binding by digest, matches side/phase, job, session, scope, checkpoint, ownership generation, harness/model-route/account/model against the row, requires this instance's enrolled identity for the side, resolves the principal's current role and capability epoch from `business_users`, demands the exact `session_handoff` grant on the binding, then mints a 60s signed permit. | Source disclosure additionally re-resolves the capture, full producer contract, native policy/workspace and enrolled destination peer. The protected copy path invokes it on source reads and target ingestion; target Core activation remains open. |

The older Workjet snapshot module records an August decision to transfer only
a context brief. That behavior must not be relabeled as satisfying the current
native session portability objective. Do not silently change the context-brief
feature into a privileged native transfer; introduce the native lifecycle with
its own explicit authorization and retire superseded execution paths only after
its acceptance evidence exists.

## Live native handoff phase RPC

The configured native Sync host now registers
`ctox.sync.session_handoff.authorize.v1` on its existing control-only
RxDB/WebRTC pool. It creates no HTTP data path, second peer, Business OS
capability token, permission grant or guest. Unconfigured/unknown bindings
fail closed.

The generated `SessionHandoffWireRequest` carries a probe or authorization
request. Both are signed Ed25519 envelopes scoped to the pinned issuer,
audience, exact generated request and fresh correlation nonce. A probe is
authorized against the actual current native gate and the binding's opposite
instance key before issuing a fresh 128-bit receiver challenge. This challenge
belongs to that exact accepted connection, sender, request, principal epoch
and binding revision. Authorization must sign it on the same connection;
consumption is one-use. A shared signaling label, role or copied signed packet
does not establish possession on a replacement connection. The bounded ledger
holds at most 64 live challenges and expires them after at most 60 seconds.

Replies are signed under the current encrypted native issuer. Native control
registration requires a control-only native pool and a mandatory publication
guard. It never grants public/pre-session access. Both room handshakes and the
exact current connection are checked at every physical response poll. The
native guard re-resolves source/target provenance and policy, holds the issuer,
policy transaction and host lifecycle fences through that bounded poll, and
rejects revoked/changed epochs, consumed/replaced challenges, expiry and host
retirement. No transaction or mutex survives Pending or an await. Rejected
setup returns the fixed auxiliary failure code, never an arbitrary private
error. Successful setup commits its identifier-only native policy audit;
publication rechecks do not append audit events per poll.

The production copy connection below consumes bounded checkpoint bytes with
retained account and policy guards. Phase replies alone are not a durable-copy
receipt or permission to stream later. Cross-store/file mutation coordination,
effect reconciliation and original-session Core activation remain open.
Native store/guard tests and signed wire tests do not establish independent-host
restoration or product acceptance.

## Protected native checkpoint copy

The running target host exposes a same-UID private operator command,
`ctox sync handoff-copy <binding-digest> <source-route>`. The route is only a
routing hint: the stored source offer pins the signing key, logical session,
checkpoint, account and ownership generation. The command uses the host's
existing admitted native peer. It does not start a second peer or move data
through HTTP or the local control socket.

Each at-most-8KiB block obtains a new exact-connection source challenge. The
target signs its current Receive decision into the Fetch request; the source
verifies that decision's signature, nonce, binding, checkpoint and expiry, then
resolves its own current Disclose grant before reading. A challenge is consumed
once even if reading or signing fails. Manifest requests and artifact/offset
requests are distinct; only artifacts named in that exact immutable manifest
can be read. Signed replies correlate the range and declared length.

The native Core credential store is resolved using the assigned workspace's
current Core configuration. Direct account-bound native OpenAI is required,
as in the existing native producer; unavailable/foreign accounts or replacement
provider endpoints deny. The account manager is retained and checks its current
credential source through source physical response polls and target ingestion
callbacks. Local issuer, policy, host retirement and pool cancellation fences
cover their respective callbacks and release before awaits. Target epochs and
binding revisions remain pinned throughout the operation.

The target stages privately and verifies complete artifact hashes before
publishing immutable blobs. It validates the manifest digest and full producer
contract, then validates all contents and portable journals and fsyncs the copy
before replying `copied:true,resumed:false`. A truncated, corrupt, timed-out or
cancelled transfer yields no successful copy response. The temporary staging
directory is dropped. Already verified immutable blobs may remain after a failed
copy; their presence alone permits neither a receipt nor execution.
The native source's enrollment still verifies full contents; repeated physical
policy checks use the manifest hash/identity instead of rehashing every blob
for every block. Receive and durable-copy checks retain full verification.

The operation is bounded to one local copy, 60 seconds, 4096 distinct artifacts,
1GiB total, 8MiB manifest and 64MiB per blob. Each exchange has a 15-second bound.
Local client disconnect retires the operation's shared publication fence;
queued blocking work checks it before writing. A fresh operation is needed
after expiry/failure; this is not an automatic retry or general crash recovery.

This connects production checkpoint sending and local durable ingestion.
It creates no Raft DATA receipt, ownership transfer, clean-effect witness or
Core activation. Target workspace preparation is connected below; original-session
Core activation remains open.
Remote policy revocation cannot atomically recall already authorized bytes:
the source sees a signed target Receive decision valid for at most 60 seconds,
while the honest target rechecks its own current grant before each request and
write. Independently replaced stores/workspace/credential files are rechecked
but do not share an atomic cross-process mutation guard. Native fixture tests
do not establish independent-host networking, installed acceptance, transfer
throughput, VM portability or external-effect reconciliation.

## Native target workspace preparation

After a successful protected copy, the operator runs
`ctox sync handoff-reconstruct <binding-digest>` against the same
running target host and private control socket. The request contains identifiers
only, never a target path, repository URL or account override. Existing copy
requests remain compatible. The enrolled source binding, target account,
host lifetime and exact target policy remain mandatory. Local reconstruction
does not require the source host to remain reachable after its protected copy;
transport still requires the exact authenticated source connection.

Both current Receive and Execute grants are required. Before staging, preparation strictly decodes the hash-verified
`native-session-state.json` against the enrolled session, harness version,
model and provider. This rejects malformed or foreign Core state without
certifying clean effects. Git reconstruction uses only the hash-verified
`native-workspace.bundle` artifact in the enrolled checkpoint.
The shared kernel imports it into an isolated bare repository with configuration,
hooks and command deadlines bounded as for normal reconstruction. It fetches no
remote objects or credentials. The exact checkpoint base is required; staged and
unstaged patches, deletions and required untracked files use the existing
reconstruction validator. The assigned target working copy is not overwritten.

Git runs in a private temporary stage without retaining an authority transaction
or account/peer lock across awaits. Authority is checked before reconstruction,
after Git IO and during final preparation publication. Revocation, changed
account/workspace/binding/epoch, host close or client disconnect
forbids publication and removes the unpublished stage. The operation retains the
existing 60-second local command bound. Successful publication writes an
identifier-only native audit and returns `reconstructed:true`, a host-created
`preparationId`, the checkpoint digest and `resumed:false`; no native path or
checkpoint contents cross the local control socket.

This is workspace preparation, not executable admission or durable quorum
evidence. Pending effects remain rejected; current native source captures still
require authoritative effect reconciliation before they become eligible.
Prepared workspaces do not create a guest, change its assigned cwd, replace a
conversation, import Core state, transfer Raft ownership or start a turn.
Preparation files and the policy audit have separate commit boundaries; a lost
reply, crash or out-of-band filesystem replacement can leave a private orphan
requiring reconciliation. Future Core admission must revalidate current policy,
workspace contents, clean effects and quorum ownership, not trust the marker.
Native regressions keep the actual captured checkpoint immutable and reject its
unreconciled effects before staging. Isolated publication fixtures exercise the
real target policy and retained account guard. A separately declared clean kernel
fixture restores exact Git state from its protected bundle after source removal.
These checks do not prove independent-host networking or installed resume.

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

The same local operator may include `providerAssignments`, a list of explicit
`ownerUserId`, `workerProfileId`, `gatewayAccountId` and `modelId` mappings.
These grants are stored in the authoritative Business OS policy store. A
current credential or sole available account does not create a grant. The
owner must be active and the profile assigned to this host. Missing assignments
permit registration only. Enrollment and command-session requests cannot issue
grants. `ctox sync revoke-guest-provider <owner-id> <profile-id>` revokes the
mapping; host restart does not restore it. Reconfiguration creates a new policy
revision, so revocation followed by regrant cannot replay an old admission.

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
need explicit reconciliation. The first-turn producer connection is described
below. Source/target handoff bindings now have separate native operator enrollment.
Original-session target resume remains open under #183. Enrollment alone does
not start QEMU or establish two-host restoration.

## Native source handoff binding enrollment

The trusted local operator runs `ctox sync handoff-enroll-source` with public
JSON on stdin: `captureId`, `targetNodeId`, `targetInstanceId`,
`targetPrincipalUserId`, `repositoryId` and `targetWorkingCopyId`.
The source host is stopped for this provisioning command. The capture must
already contain a complete native checkpoint. Session, execution ownership,
account, model, capabilities, source principal and source workspace are
resolved from the actual immutable capture, never supplied by this request.
Current project/chat/profile/computer/provider/workspace policy must exactly
match its captured revision. The native directory identity and complete
manifest/blob/journal hashes are verified again. Journal-only captures,
changed assignments, foreign roots and unavailable artifacts fail closed.

The target public identity is resolved from the configured native Sync
membership. It must be another enrolled peer that can replicate and execute.
The target instance, principal, repository and working-copy IDs are explicit
operator-selected references; they do not prove target-local entitlement and
never become paths or credentials. A deterministic digest binds these choices
to the actual source facts and both peer identities. Exact retry returns the
same binding; a revoked or conflicting row cannot be resurrected.
`ctox sync handoff-revoke <binding-id>` revokes this preparation.

Enrollment writes no permission grants, target bindings, durable-copy receipts
or clean-effect witnesses. The production disclosure gate requires the exact
separate disclosure grant and re-resolves the current source facts, full
capability set, ownership node/generation and destination peer before signing.
Provider assignment is checked here; a currently held credential/account guard
and a physical publication fence are held by the native copy path above.
A permit is not permission to stream later without those retained guards.
Enrollment, revocation and every gate decision use the existing Business OS
event store. Decisions commit their identifier-only audit under the same held
issuer/policy transaction; failure to persist the audit denies the permit.
Request payloads, nonces, journal text and reusable credentials are excluded.

Creating or changing a permission grant increments the principal capability
epoch, invalidating the original capture policy. The complete native capture
now retains its immutable policy snapshot in the private native store, hashed
against the captured policy revision. After the exact disclosure grant and
separate explicit current provider/workspace regrants, the trusted operator
may run `ctox sync handoff-reauthorize-source <binding-id>`.

Reauthorization compares that actual capture snapshot with current policy.
Only monotonic epochs, assignment revisions and provider timestamp changes
are eligible. Role/active state, project/chat/profile/computer records,
account/model, working-copy record/path/device/inode, producer contract,
checkpoint and both peer identities must remain the same. Missing historical
proof or a changed scope requires a fresh capture. The command neither
lowers epochs nor creates grants or assignments.

The stable binding ID retains its exact grant scope. A successful renewal
changes the binding digest and increments its revision atomically with the
private current-source authorization, policy snapshot and audit event. A
later renewal must also preserve scope and advance monotonically from this
last authorization. Old digests cannot mint permits; exact retry leaves the
digest/revision/audit unchanged.
The production disclosure gate re-resolves this current authorization on
every call. Revoked bindings cannot be renewed. The actual stopped Core
regression now follows grant mutation, current assignment provisioning and
native renewal with a strictly increasing principal epoch.

This is source policy reauthorization. Current credential/physical-byte
guards and same-session target activation remain open. Target-local enrollment
is described below; Receive/Execute still require separate exact grants. Effect
reconciliation, authenticated byte transport and target activation remain open.

## Native target handoff enrollment

Target policy is now enrolled through the trusted local operator, separately
from source disclosure and guest/process creation. Provision the target's
existing project and logical chat, assigned worker profile/computer, explicit
provider account/model and protected native working copy first. The project
and chat IDs remain the source IDs; neither a target cwd nor a provider session
label can create or select a replacement conversation.

`ctox sync handoff-configure-target-repository` accepts public JSON with
`ownerUserId`, `workerProfileId`, `projectId`, `workingCopyId`, `repositoryId`.
It binds that opaque logical repository to the already assigned native working
copy under current principal authority. It cannot supply a path or issue a
handoff grant. Repository reconfiguration advances its native revision; grant
changes require explicit current provider/workspace/repository regrants.

The target runs `ctox sync handoff-target-challenge`. The source runs
`ctox sync handoff-source-offer <binding-id> <challenge>`, after current source
disclosure authorization. Its public metadata contains the actual captured
execution facts, current binding revision, original logical chat, exact target
selection and both enrolled public identities. The existing generated Disclose
permit signs the canonical metadata hash plus this fresh target challenge.
It contains no native paths, journal contents or reusable credentials.

The target runs `ctox sync handoff-enroll-target` with `{offer, workerProfileId}`
on stdin. It checks its own enrolled instance/key and current source membership,
the full metadata/digest/signature and 60-second validity, then consumes its
exact issued challenge in the same transaction as target binding, native
provenance and identifier-only audit. Changed/expired/reused challenges, unsigned
or altered metadata, a foreign target, missing local principal/project/chat/
computer/account/workspace/repository assignment and an audit failure deny.
Exact lost-response retries remain idempotent. A revoked target binding cannot
be resurrected. Explicit fresh enrollment may refresh current target policy
with a new local binding revision; it creates no grants or running guest.

Receive and Resume gate decisions now require that native target provenance
and re-resolve all current local assignments, configured computer/requirements,
workspace directory identity, exact binding fields, logical chat and provider
contract. A copied legacy binding row without enrollment cannot authorize.
Account/profile/role/epoch/repository/policy changes require explicit renewal;
row edits or a rotated issuer cannot replace the enrolled proof.

This is native provisioning over local operator CLI, with signed public source
metadata. It is not the authenticated peer checkpoint byte transport. No HTTP
bridge, protected journal stream, remote credential use, reconstruction, quorum
receipt, ownership takeover or target Core activation is supplied here. The
future transfer must obtain fresh decisions from both live instances and retain
account/policy/accepted-connection guards at physical publication/ingestion;
the stored offer is historical enrollment provenance, never current source
permission. These are compatible same-contract mappings; changing harness/model
needs the later typed transition over the same durable conversation.

## Native working-copy source checkpoint

The trusted local operator may add `workspaceAssignments` to
`ctox sync configure-guests`. Each entry contains `ownerUserId`,
`workerProfileId`, `projectId`, `workingCopyId` and `nativeWorkspace`.
The existing owner/profile/computer/project and active working-copy records
must agree. The Workjet working-copy path stays opaque: only this explicit
host assignment supplies the native path. The path must be absolute, canonical,
owned by this process user and protected from group/world writes. Its directory
device/inode, current principal epoch and assignment revision are persisted in
the existing policy store. Provider and workspace grants are configured in one
policy transaction; invalid input rolls back both. Host startup reads this
authority and never recreates a revoked grant. The local operator can revoke
it with `ctox sync revoke-guest-workspace <owner> <profile> <project>`.

The assignment participates in the admitted policy revision. Before native
TurnStart, its path must equal the actual turn cwd. A configured native
producer holds an exclusive advisory lock on the actual directory from
controller binding until its checked, quiescent source publication finishes.
Another cooperating native writer cannot acquire that directory concurrently.
This lock does not fence arbitrary editors or filesystem changes outside
native ownership; directory replacement is independently rejected.

After checked Core shutdown, the actual queue capture owner calls the shared
CheckpointStore capture under its retained worker/account/policy/controller
guards. Bounded local Git IO retains the base, staged and unstaged patches,
deletions, untracked files and an actual HEAD bundle, including locally
unpublished commit objects. The protected native artifacts also retain the
actual journal, final Core configuration and sealed session/provider state.
The artifact store must be outside the captured working copy. The bundle uses
at most two packing workers; every Git command is bounded and the complete
capture has a ninety-second deadline and the existing per-blob budget.
Private metadata links the exact capture and workspace revision to the
immutable manifest. A conflicting retry cannot replace it.
The shared checkpoint validator accepts the canonical native CTOX Core writer in
addition to legacy Codex journals. Native journal metadata must match the
manifest's writer version and model route on capture, publication and load;
identity, syntax, limits and unsupported-harness checks remain in force.


Ordinary enrolled chats without a native workspace assignment remain explicitly
journal-only and emit a checkpoint-unavailable reason. Their working-copy
labels or turn cwd cannot grant checkpoint capture. A configured but stale,
foreign, replaced, revoked or non-Git workspace fails visibly.

Native turn external effects remain unknown, so the manifest retains an
unresolved effect and the existing reconstruction guard refuses execution.
This is source capture, not effect reconciliation, permission to disclose,
target authorization, upstream retention or same-session target activation.
Attachments/files outside the captured Git working copy and complete VM state
still require their actual native owners. No target transport or live two-host
acceptance is claimed by these source regressions.

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
account and existing selected model, both matching the explicit owner/profile
assignment before client startup and again after the startup await. Held policy
checks fence later effects after account assignment or principal epoch changes.
It cannot substitute a proxy, local model,
external executor or worker-profile route. Unenrolled tasks and canonical
external-executor commands retain their existing execution owners. Tasks that
have no existing signed Business OS MCP scope do not acquire one from guest
enrollment.

This connects the first-turn production producer only. It grants no checkpoint
disclosure, target receipt or target execution. An already-bound guest cannot
start a replacement provider session without lifecycle reconciliation.
Source/target handoff enrollment and complete native working-copy capture are
described above. Authorized checkpoint transport, external-effect reconciliation
and continuation of the original session remain open.

## Queue-owned source journal publication

The actual queue turn owner now consumes its native producer after a successful
reply, before persisting the successful worker-attempt/assistant marker and
before the service can retire its lease. Both the ordinary context path and
the plain-prompt path invoke checked native quiescence. A failed or ambiguous
shutdown, missing journal, stale command/account/policy/lease or invalid journal
rejects that completion; it cannot fall back to a fresh session.

The retired Core journal reader and exact capture-only provider owner publish
the original validated bytes through the shared Sync kernel's content-addressed
`CheckpointStore`, under a host-created private source directory. The native
policy table `business_native_source_journals` stores the authority binding,
artifact reference and private store location, not another journal payload.
Renderer/model requests cannot supply that location. Publication holds the actual worker,
account, policy and guest-controller guards. It binds the source instance,
canonical owner/profile/project/chat, actual job/provider session, ownership
generation and admission policy revision. A retry returns the same receipt only
for identical bytes and bindings; conflicting input requires reconciliation.
Receipts and progress events contain identifiers/hashes, never journal payloads.

The capture owner also reads the final `ThreadConfigSnapshot` from that same
retained Core thread after checked client shutdown, before draining its owned
runtime. Failed shutdown never invokes the exporter; export failure still drains
the runtime and rejects capture. Model/provider/ephemeral state must match the
admitted native producer; the source workspace must be an existing canonical
directory. The bounded private Core-configuration artifact preserves actual
model, reasoning/personality, approval/sandbox, service-tier and session-source
settings. It includes only the admitted account reference, never authentication
material or command-session credentials. Its metadata is associated with the
same capture in `business_native_source_core_configurations`; exact retry
succeeds and changed configuration cannot overwrite it.

This configuration is source input, not proof of a remote provider checkpoint.
It explicitly retains unresolved provider continuation and unknown external
effects. Its source workspace is not target path authorization. Target-local
account/workspace resolution, permissions and native resume must still establish
the actual continuation. Older journal-only captures are not promoted into
configuration or full checkpoints by migration. No transport contract changes
or disclosure grants are introduced by this private artifact.


The same checked owner now exports a sealed native session-state artifact
from the actual stopped Core Session, rather than constructing provider state
from journal syntax. It retains the actual post-compaction history, reference
context, token usage and previous-turn settings, instructions/tools, and the
ModelClient's completed request/response cache and fallback state. Capture
requires successful session-loop termination, the recorder receipt and no
active turn. Pending or lost response receipts reject export. Settings and
state are captured together under the Core state lock and then published under
the existing native guards. The existing CheckpointStore holds the bytes;
`business_native_source_session_states` associates their reference with the
exact journal capture.

These bytes are protected model-visible content and may contain private
instructions or response IDs. AuthManager, dependency environment, approval
grants, sockets and per-turn routing tokens are excluded. Transfer must use
the authorized protected consumer; target-local credentials, workspace and
execution rights must be resolved again. The source snapshot preserves input
for actual continuation but does not prove upstream response retention or
target restoration. External effects remain unknown; legacy captures gain
no synthetic state or permission.

Native capture retires model execution. If the existing continuity mechanism
requests another turn, its durable refresh demand remains pending rather than
invoking this retired producer or creating a replacement. Ordinary producers
and externally supplied sessions retain their existing owners.

This is durable private journal input, not a complete checkpoint or a disclosure,
receive or execution grant. No provider-state blob is fabricated from journal
syntax. Workspace/files/attachments, provider continuation and external effects
still require their native owners. Source/target enrollment is described above;
protected transport and actual target activation remain required. Artifact fsync,
the policy transaction and assistant/worker store have
separate commits: failed metadata publication can leave a private orphan blob;
a later authority or reply-persistence failure can leave private journal
input without a successful worker marker and needs reconciliation. Installed
model/VM/two-host acceptance remains separate from the source storage regressions.

## Native producer lifetime

The production first-turn connection preserves three separate owners:

- `service::run_foreground` retains the configured `sync_host::ServiceHost`
  for daemon lifetime. Guest configuration makes that same host construct its
  registry and attach the source to its actual native peer.
- `execution::agent::turn_loop` calls
  `start_native_guest_with_business_os_mcp` only for an enrolled, currently
  scoped chat/Crew assignment. A signed command token alone cannot supply
  that assignment or keep a stopped host peer live.
- `PersistentSession::run_turn_async` owns `NativeProviderTurnOwner`. An
  exact successful TurnComplete can retire that owner into a distinct capture
  owner retained by the persistent session; other returns revoke it. A
  `NativeGuestExecution` observation handle cannot extend provider authority.
  A subsequent turn requires lifecycle reconciliation, rather than creating
  a replacement provider session under the old guest assignment.

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
an empty or fabricated provider-state blob. The first-turn factory now has its
queue caller; the complete checkpoint capture and handoff consumer remain open.

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
quiescence boundary. The first-turn queue connection supplies a producer, but
does not yet invoke CheckpointStore::capture. The returned handle is not fresh Raft
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
5. Binding rows come from the separate native source/target enrollment paths
   above. The target verifies a signed offer against its current local policy;
   enrollment neither supplies grants nor starts protected transfer.

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
ownership. Source capture and native source/target enrollment now exist; the
actual sender/receiver/resume consumer remains required. Gate regressions use
the production constructor with signed source offers and real target-local
assignments for key deletion/rotation, epoch changes, competing issuer/policy
writers and absent/uninitialized stores. The stopped Core capture regression
also exercises source-offer signing, target enrollment, exact retries, changed
local authority and workspace replacement. These are policy regressions, not
real session-transfer acceptance.

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
Native enrollment is described above. The held publication guard and actual
sender/receiver remain required; this reconciliation does not supply them or claim acceptance.

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
