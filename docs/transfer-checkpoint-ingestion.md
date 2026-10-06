# Authorized transfer-to-checkpoint staging

The Unix daemon adapter `transfers_checkpoint::stage_transferred_guest_checkpoint`
consumes completed native transfer jobs and returns the canonical
`ctox_sync::guest_restore::StagedGuestRestore`. It is a native integration
function, not a renderer command or a production guest registration.

The caller supplies the native execution authority, lifecycle owner, enrolled
guest identifier, execution ownership, protected checkpoint digest and original
manifest/artifact transfer identifiers. The destination is independently resolved
by the lifecycle owner; no caller-selected guest path is accepted.

Before ingestion, the adapter requires current local execution ownership, its
protected digest and a checkpoint that does not require refresh. Completing an
external effect does not make an older checkpoint fresh. The canonical staging
and import checks still revalidate current authority after the adapter preflight;
only the native CommitEffectCheckpoint transition can publish the newer durable
copy and complete its pending effect atomically.

Every job must retain the manifest's source identity and original enrolled
account. Each file retains its own original grant. The real PeerRangeSource
authorizes each saved immutable request before and after reading, again before
checkpoint publication and after staging. Completed cache entries do not bypass
those checks.

Store::read_completed_peer_artifact verifies native receipt provenance, expected
size/hash, regular-file identity and the saved job around consumption.
CheckpointStore independently hashes the bytes actually consumed and durably
publishes content. Canonical manifest hashing, exact artifact coverage, supported
portable journal validation and absence of unresolved effects are required.

The native handoff contract also requires owner-signed Disclose permits for
ProtectCheckpoint and CommitEffectCheckpoint, and target-signed Resume permits
for takeover. These bind the exact request, job, checkpoint and ownership
generation; takeover requires the same binding digest. File/account grants and
completed transfer receipts cannot replace these phase decisions.

Before production use, the native owner must enroll the binding from trusted
workspace/account authorities and supply held Receive/Disclose authority around
every protected chunk and publication boundary, plus current Resume authority
after reconstruction. The existing permit-returning helper alone does not hold
that authority. This adapter does not create binding rows, sign permits, submit
protection/takeover commands or provide that missing sender/receiver integration.
See [the native handoff integration boundary](ctox-sync-handoff-integration.md).

The returned stage is immutable and inspection-only. The native owner must use
commit_guest_restore under the actual controller and live execution/attempt
fences. Git/provider reconstruction writes into a separate native-owned runtime
path after verified import, before actual guest readiness. A returned stage or
content receipt is not an execution, takeover or future grant capability.
Publication, reconstruction and readiness failures retain the canonical
reconciliation semantics described in [native guest restore](native-guest-restore.md).

The component tests exercise actual completed Store jobs and CheckpointStore
files with a deterministic peer fixture. They cover independent staged/unstaged,
untracked and provider contents, revoked or foreign accounts, exact artifact
coverage and tampered cached bytes/receipts. Staging cases cover stale checkpoints,
invalidation after preflight, unpublished stage cleanup and post-stage revocation.
The legacy fixture's absent disclosure permit cannot authorize native takeover.
These are not real WebRTC, two-host Git reconstruction, provider continuation or
installed workflow evidence. Those acceptance obligations remain open.
