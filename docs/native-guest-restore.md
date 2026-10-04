# Native guest checkpoint import and readiness

The native API is `ctox_sync::guest_restore` (Unix only). It extends the existing
content-addressed CheckpointStore and ExecutionAuthority; it does not add an
execution protocol, renderer endpoint or HTTP business-data bridge.

`GuestRestoreOwner::resolve_destination` resolves an enrolled guest to its native
instance, human owner, project, thread, worker profile, controller identity and
generation, and a canonical private import parent. The request does not supply a
destination path. Native ownership/policy lookup is mandatory: those fields are
claims until resolved by the lifecycle owner, not authorization in themselves.

`stage_guest_restore` requires the current local execution owner and a protected
checkpoint. It compares checkpoint sequence and the entire portable session
contract (scope/session/harness/version/model route/gateway account/model and
required capabilities), rejects pending effects, and revalidates after private
materialization. Artifacts are rehashed by CheckpointStore. Nested files and
directory entries are flushed without following symlinks. Dropping a stage
removes only its private unpublished tree; existing user files are untouched.
The stage retains an open, non-symlink import-parent directory handle. Its
device/inode must still match the native pathname after staging, before effect
admission and inside the publication guard. Replacing a private directory at the
same pathname, even with identical staged bytes, denies publication. Parent
durability is flushed through the retained handle. The native owner must still
serialize filesystem publication and directory mutation under its real guard.

`commit_guest_restore` admits one BeginEffect through the existing committed
authority. Replayed or rejected receipts never dispatch the publication again.
GuestRestoreOwner::with_current_fence must invoke publication exactly once under
the REAL shared controller AND live execution/attempt guard, with all
revoke/takeover/expiry/shutdown paths using that same guard. A preflight boolean
or an independent mutex is insufficient. The callback reserves a fresh import
name only after revalidating the exact manifest layout, file hashes, lengths,
types, symlink targets and executable state of the immutable staged payload.
It flushes the final nested tree under the same publication fence, publishes
the stage and flushes the parent. Missing or added entries and changed content
deny publication. The native owner must exclude concurrent mutation throughout
this callback. A failed/cancelled/uncertain
operation after admission leaves the effect pending. Recovery must reconcile its
actual files and old process/effect stop witness; it must not automatically retry
or mark completion from directory presence. Normal authority TakeOver and Stop
already reject unresolved effects. The returned GuestImportReceipt binds the
full native destination, execution contract/ownership, digest, sequence, target
and effect. It establishes imported state, not guest startup or execution rights.

`confirm_guest_ready` revalidates current quorum ownership, protected checkpoint
and completed import effect. GuestReadinessOwner::with_live_guest must resolve
the import against canonical registration and observe the actual retained guest
process, guest session and endpoint while holding the same current native
fences. It returns GuestReadyReceipt only after that observation. Image presence,
a successful file transfer and QMP "running" cannot satisfy it. The receipt is a
current observation, not a reusable capability: subsequent input/observation
still uses the native guest authorization and frame lifetime.

## Integration responsibilities

The staging path is inspection-only. Git/provider reconstruction writes to a
separate native-owned runtime path after verified import and before readiness;
it must not modify the signed checkpoint payload or treat reconstruction as
execution authority. Reconstruction failures retain the verified import for
reconciliation and cannot synthesize a ready guest.

- Architecture owns this staging/import/readiness orchestration and component
  checks. Publication must run on the native owner's supervised blocking
  boundary; the existing CheckpointStore performs synchronous file I/O.
- VM owns the concrete native destination registry and GuestRestoreOwner /
  GuestReadinessOwner implementations, actual provider/Git reconstruction,
  retained prepared QEMU process and real endpoint handshake, controller and
  delivery lifecycle, and guest command registration.
- Crew owns attachment of the exact live task/harness attempt guard, worker
  revocation and uncertain-effect/stop-witness reconciliation. BeginEffect alone
  does not prove that remote worker revocation serializes with a local effect.
- Transfer owns obtaining authorized checkpoint artifacts over the existing
  native P2P plane and binding its original source/account/grant on resume.
  desktop_files grants and workjet-transfer Git reconstruction grant no takeover.
- Workjet Main owns the single composed installed client and whole workflow
  acceptance; this API does not enable the VM capability catalog.

The native producer constructor is
`PersistentSession::start_native_guest_with_business_os_mcp`. It requests a fresh
persisted harness thread before any model turn, retains the actual compiled
harness/version and configured model route, and pins the account resolved from
the configured native credential store. Its current supported profile is the
direct authenticated ChatGPT Responses route; API-key, local and proxy-selected
accounts are rejected because their account binding is not implemented.
The provider callback holds the local account guard and rechecks the credential
store, detecting external logout or a foreign account without relying on a cached
startup snapshot. This read does not atomically fence external credential or
SQLite policy mutation. Verified command authority is checked before and after
admission; an uncertain or rejected admission poisons the session. A generic
legacy provider witness cannot acquire guest admission.

The constructor and its account checks do not replace canonical instance,
project, principal, worker-profile or controller resolution. The concrete
PendingCreate / quorum Create / fresh native policy revalidation / Admitted
consumer and production registration remain unfinished. No capability is enabled
by these producer changes. New account, durable-thread and provider regressions
are source coverage until executed on the composed revision.

There is deliberately no default/permissive production lifecycle owner or
readiness probe in this module. Production registration remains unavailable
until VM/Crew connect those actual authorities. The component tests exercise
real checkpoint files and the deterministic authority effect ledger, with a
test lifecycle owner; they are not real quorum, provisioned-guest, two-host or
installed application acceptance. Required follow-up is the concrete native
owner integration, stale/foreign account and controller/worker-revocation races,
actual prepared guest/provider restoration, and two-host relocation/ready/input.
