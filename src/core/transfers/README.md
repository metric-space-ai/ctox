# CTOX durable transfers (first slice)

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

The adapter tries each source once per attempt using one engine connection.
All candidate URLs bind to the same caller-supplied content identity. Unreachable,
truncated, wrong-length or wrong-hash sources fall through to the next mirror;
partial bytes are never mixed between sources. Completed rejected payloads are
quarantined so explicit resume can retry repaired sources. Quarantine is bounded
at sixteen rejected payloads per source, then requires operator cleanup. Only a
complete verified source can produce a receipt. Parallel multi-source assembly
remains open. An accepted cancellation cannot be reversed by a late pause/resume.

## Engine source and limits

The private Git dependency is `mkh-welsch/aria2-rust` at
`b325b0895eaec60795cf462a2bd1ca7cee5878e5`, based on remote main
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
Peer jobs will reuse that facility; signaling remains rendezvous/control.

Existing `business_os/workjet_transfer_git.rs` owns Git bundle/patch/untracked
packing and staged apply. Reuse that manifest/receipt boundary for the subsequent
worktree slice rather than copying `.git` pointers or inventing a second format.
Execution fencing remains Crew-owned; destination capabilities are VM-owned;
Workjet owns move/continue UX. A directory transfer is not a resumed harness run.

## Verification and remaining work

`tests/download.rs` exercises real loopback HTTP, durable receipt/reopen/idempotency,
wrong hashes/lengths, exclusive worker ownership, pause/resume, terminal cancellation,
mirror isolation/failover, rejected-prefix recovery and publication-crash recovery. Run with two test workers via the shared admission gate on the Mac:

```
cargo test --manifest-path src/core/transfers/Cargo.toml -j 2 -- --test-threads=2
```

The first macOS verification stopped during engine compilation at Linux-only
socket options; no tests ran. The pinned repair still requires a new run of the
suite and native integration checks. Directory durability is implemented for Unix only; Windows
activation explicitly fails instead of issuing an unproven durable receipt.
No platform is claimed accepted yet. Native command/progress projection through
CTOX Sync, peer capability-scoped requests, peer interruption/resume, parallel
multi-source assembly, Git worktree integration and DevOps two-host acceptance remain open.
