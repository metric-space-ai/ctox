# CTOX durable transfers

CTOX owns download intent and lifecycle in `runtime/ctox.sqlite3` (`ctox_transfer_jobs`).
The service owns a single transfer worker protected by an OS file lock. Closing a
Workjet window cannot stop it. A new worker reconciles interrupted `running` jobs
only after obtaining that lock. Failures require explicit resume; cancellation is
terminal and retains partial bytes. Request IDs bind the entire immutable request.

Local operator interface, with the daemon running:

```
ctox transfer download ID LOWERCASE_SHA256 SIZE HTTP_URL [MIRROR_URL...]
ctox transfer status ID
ctox transfer pause ID
ctox transfer resume ID
ctox transfer cancel ID
```

This initial interface accepts public HTTP(S) URLs without credentials, queries or
fragments. It never reads `.netrc`, accepts engine hooks/options, or chooses a user
workspace destination. Authenticated sources need typed secret-store resolution.
Do not expose this local CLI as a remote command without the existing native policy
gate and source/destination authorization.

The private `runtime/transfers/staging/ID/source-N/payload` and engine `.aria2`
control file hold resumable input independently for each source. Content length is checked before allocation and enforced on
writes. SHA-256 and length are checked before publishing `objects/SHA256`; files
and directories are flushed before a receipt commits. An interrupted publication
is recovered by re-verifying the object. Existing objects are never overwritten.
Receipts identify content and engine revision; they grant no execution authority.

For multiple candidates, the adapter first assembles ranges with at most two
connections in a separate `staging/ID/combined` directory. Candidate length and
range support are checked, and the caller-pinned SHA-256 must match the complete
assembled object. The engine owns all range futures and records a progress bitmap
before parallel writes; even an empty loaded bitmap takes precedence over sparse
file length on resume. Pause settles writers before the worker lease is released.

A failed assembly falls back to one independent connection per source, trying
each candidate once. Unreachable, truncated, wrong-length or wrong-hash sources
fall through to the next mirror. Combined and independent partials never mix.
Completed rejected payloads are quarantined so explicit resume can retry repaired
sources; quarantine is bounded at sixteen payloads per staging directory, then
requires operator cleanup. Only complete SHA-256/length-verified bytes can produce
a receipt. An accepted cancellation cannot be reversed by a late pause/resume.

## Engine source and limits

The private Git dependency is `mkh-welsch/aria2-rust` at
`8364bcd7902dbd853a0f746c3dc937bbaadec561`, based on remote main
`7bfacc2cf27e55d4755b06623c1b997880d0c697`. Its LICENSE and manifest declare
GPL-2.0-or-later. The patch adds optional `ctox-expected-length` checks before
allocation and at all storage write entry points, with a direct boundary test.
It also guards Linux-only QuickAck and uses SO_NOSIGPIPE on Darwin for socket
writes; a regression test exercises scalar/vectored TCP and UDP output.
Private source has not been copied into this public repository. Authenticated
build access is required; public CI/release source distribution remains unresolved
pending the owner’s publication decision. The upstream README is historical and
contradicts its newer FEATURE_MATRIX; neither is independent acceptance evidence.

The engine's `room.rs` uses multicast UDP discovery and HTTP with a shared room
password, and offers path/size without a content digest. CTOX does not enable that
listener or discovery. Existing CTOX `rxdb.file.fetch` already offers authorization,
range requests, bounded chunks, cancellation and transport backpressure over WebRTC.
Peer jobs reuse that facility; signaling remains rendezvous/control.

Native job creation captures the issued non-secret grant ID, enrolled target ID,
durable native account generation (not Electron Main’s process-local epoch), and a
SHA-256 fingerprint of the existing native principal contract (including device
and authorization epoch) inside the immutable request. These are non-secret
binding metadata, not credentials or grants. Resume never replaces that snapshot
with a newly logged-in account. Legacy requests without a snapshot still decode
for diagnosis, but the enrolled native adapter rejects them. Grant IDs are also
immutable: a restart cannot silently mint or select a replacement grant. The
adapter requires an explicit grant-admission checker with no permissive default;
current account/file permission alone cannot validate an arbitrary grant ID.
The native source implements permission-checked grant issue/lookup/revoke and
nonce-bound device validation in the existing CTOX policy/secret stores. The
grant ID itself carries no authority. Grant and account replies use the guarded
auxiliary dispatcher and recheck current authority at each physical send poll;
see [`docs/native-transfer-grants.md`](../../../docs/native-transfer-grants.md).

The enrolled adapter checks current host enrollment/account, a fresh signed peer
principal, and the existing remote file-fetch policy using an empty range before
cache reuse and publication. Account state is checked again after the exchange.
The production daemon constructs a NativeTransferAccountHost and daemon-owned
NativeTransferPeerResolver. The lease-owning worker retains its query database
and native session across UI disconnects and drains them during bounded shutdown.
Pairing, grant admission and peer downloads use the native command/service path.
Historical passing checks below apply only to their recorded revisions; final
runtime verification and installed two-host acceptance must identify their source.

Existing `business_os/workjet_transfer_git.rs` owns Git bundle/patch/untracked
packing and apply. Its index extension preserves staged and unstaged changes
separately within that same manifest/receipt boundary; see
[`docs/transfer-git-index.md`](../../../docs/transfer-git-index.md) for rollout
and legacy semantics. Never copy `.git` pointers as a worktree transport.

Execution fencing remains Crew-owned; destination capabilities are VM-owned;
Workjet owns move/continue UX. A directory transfer is not a resumed harness run.

## Native peer jobs

`DownloadRequest.peer_source` binds an instance/key, collection and file ID;
HTTP sources must be empty for this form. These fields are immutable claims,
not authorization. `worker_with_peer` requires a native-owned range resolver;
the ordinary HTTP-only daemon rejects peer work when no resolver is installed.
The native binding verifies the enrolled source proof on its admitted Sync
connection and then consumes the existing `rxdb.file.fetch` range API. It requires
a daemon-owned admission hook for the exact immutable job and connection. That
hook must consult current enrolled account/epoch, session generation and file
policy; source identity and a previous receipt cannot substitute for admission.
The worker checks admission before cache reuse, each range and publication,
including fully checkpointed resumes, and cancels pending admission on pause or
cancel. Range reads carry the complete job identity. The production daemon hook
and command/session registration are connected through transfers_native and the
native account host; real two-host execution verifies their installed behavior.

Peer ranges are at most 1 MiB. The worker flushes each range before committing
its offset to the existing SQLite job. Reopen truncates uncommitted tail bytes;
missing/truncated staging resets the offset. A complete SHA-256/length check
still precedes immutable publication. Rejected content is quarantined and the
offset reset for explicit retry. Peer receipts identify the WebRTC transport
and carry no aria2 revision. This provides no checkpoint protection, execution
handoff or guest readiness. Those require the existing native authority APIs.

The peer-store interruption/reopen, identity and authorization tests use a range provider
fixture; they do not prove native networking or two-host acceptance.

## Verification and remaining work

`tests/download.rs` exercises real loopback HTTP, durable receipt/reopen/idempotency,
wrong hashes/lengths, exclusive worker ownership, pause/resume, terminal cancellation,
mirror isolation/failover, rejected-prefix recovery and publication-crash recovery.
`tests/peer_authorization.rs` covers cache reuse across jobs, missing admission,
revocation before publication/fully checkpointed reopen, and cancellation during
admission. Run Linux checks through the shared gpu3 build lane, retaining one
stable task name for the PR so source and compiler caches are reused:

```
~/.codex/bin/gpu-build-run.sh --owner THREAD_ID --task ctox-pr227 --src WORKTREE -- \
  cargo test --locked --manifest-path src/core/transfers/Cargo.toml -j 6 -- --test-threads=2
```

Mac-only build and acceptance steps use the Mac admission gate.

The macOS run on 2026-09-27 passed all 27 targeted checks: five native file
and seven shared query-reader tests on native `ff5b233225`, nine HTTP download
and four peer-provider tests on adapter `6292318bee`, and two engine
socket/storage regressions on engine `85d4eda67`.

The standalone harness uses the exact archived private dependency via a local path.
It verifies known-empty content and rejects truncated HTTP responses, retaining
the received prefix for resume. Earlier platform and framing failures were repaired.
The later engine-only run on `aea4d55e` passed all eight bounds/socket and
real HTTP mirror/sparse-resume tests in 58.102 seconds. Adapter `c8279d353`
passed all twelve HTTP and four peer-provider tests in 14.738 seconds, including
distinct range contributions, corrupt assembly fallback and pause/reopen fetching
only missing ranges. Exact `0d0bc6ef5` subsequently passed all seven peer-provider
tests, the existing HTTP offline-publication recovery test and the updated native
binding compile check in one bounded 87.451-second run. The native check emitted
only unused-item warnings from the isolated harness; it does not exercise a real
daemon account-admission implementation. Full root integration and installed acceptance remain
unverified. Directory durability is implemented for Unix only; Windows
activation explicitly fails instead of issuing an unproven durable receipt.
These historical component checks do not establish platform acceptance. Native
command/progress projection, capability-scoped peer requests, interruption/resume
and Git worktree packing/apply are implemented. Acceptance requires the installed
Mac and gpu3 workflow, original account/grant continuity and exact Git staging
and file fidelity; a directory transfer alone does not resume execution.
