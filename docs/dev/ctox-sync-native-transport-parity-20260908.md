# Native transport validation parity — 2026-09-08

Status: **production acceptance remains incomplete; PR #69 is a draft**.
At source `79ee5a2a4b154d6e837c4a158ef2602d931c1a6c`,
[run 34244920380](https://github.com/metric-space-ai/ctox/actions/runs/34244920380)
passes both native platforms, all 122 JavaScript/browser checks without skips,
12 command-completion tests and six command transaction tests. Four actual
host processes and the 21-collection reload scenario pass. The real browser
command budget still **fails**: 30 complete samples, p50 **419.5 ms**, p95
**620.15 ms**, minimum 266 ms and maximum 637 ms. Reloads take
35.378 / 43.057 / 45.688 seconds, with revision 1 to 23, 36 to 19 live leads,
3,911 thread states and 11,130,431 source bytes preserved across native restart.
The exact built merge is `5b71236ba8023f3d5fb2dd81163784d44cae7b59`; binary
SHA256 is `184f540331b64132fc28a2eaae7aae09baadfc3c6e6eb0dbda2110b9355bd9e6`.
The strengthened rollback-trigger test added after this source is not covered
by this result.

The subsequent [run 34246138597](https://github.com/metric-space-ai/ctox/actions/runs/34246138597)
at source `0f798f17300c6ed47d81e5231ed4ba361fe67542` passes both native
platforms, the 12 completion tests and six transaction tests, including the
stronger linked-projection rollback trigger. Its warm budget remains red:
30 complete samples, p50 **432 ms**, p95 **544.35 ms**. All 21 collections
complete in 34.593 / 43.070 / 18.633 seconds across the reloads.
The exact built merge is `b40cd16590f984746eeb11aa7f37e3a515b9c760`;
binary SHA256 is
`ad3f9c68a28bb815f4eeb96ea19c940429bf4a80beb4960bce3e9eca4f485e7b`.

Main `365927a3ce666927d3b9173626f567673f6f98cb` is integrated before the
current context-submit change. The global context form now presents its pending
chat through the existing chat submission handler before backend acceptance;
the old dispatch-then-open sequence is removed. Crew identity, command ID,
authorization and object context are retained. The targeted facade test and
75 existing chat tests pass locally, as do the static shell contract and
data-plane guards. These are not end-to-end acceptance.

CI now includes the existing shell geometry and chat behavior harnesses and
the real browser/native context action scenario, with a new **150 ms**
submission-to-visible-prompt gate. This gate and the current source's actual
browser/native behavior remain unverified. Local browser and Rust runs are
deferred because the shared capacity gate rejects insufficient tmp space and
high swap use. No tenant is upgraded; PR #69 remains draft. Generic draft/open
hydration, database lifecycle recovery and the architecture-wide acceptance
criteria remain open.

The command-scoped writer and deferred queue-store attachment preserve core
claim/idempotency, outbox and projection ordering. Their 30-sample subphase
medians are initial RxDB projection 49.293 ms, core completion 8.741 ms,
canonical read 4.899 ms, local projection 6.781 ms and final RxDB projection
2.403 ms. The preceding [run 34240025961](https://github.com/metric-space-ai/ctox/actions/runs/34240025961)
at `d7eea14a213bf1bc66ca38f6e70fd378a26d4f99` measured core completion
68.714 ms and final RxDB projection 66.545 ms, with overall p50 447 ms.
These subphase reductions do not establish the required end-to-end p50 below
300 ms or critical-boot p95 below five seconds.

The latest run also exposes corrupt diagnostic framing in the browser harness:
it prefixes arbitrary stderr chunks, which can split a JSON key or command ID.
Initial/final projection diagnostics have 30 intact samples; the older intake
and queue diagnostic streams do not. Incomplete parses are not valid 30-sample
statistics. The harness now decodes and prefixes complete stdout/stderr lines,
also recognizing a listening marker split across chunks. A focused stream
regression passes for one-byte UTF-8/JSON chunks, multiple records per chunk,
CRLF and a final line without a newline. Full browser verification of this
harness change remains pending; the performance limit is unchanged.

The preceding complete full-host run,
[34237292653](https://github.com/metric-space-ai/ctox/actions/runs/34237292653)
at source `5eeffaa40ad6147dd14c46f47c0282a5d2f158da`, passes four real CTOX
hosts and all 21 browser collections across three reloads. The warm command
budget **fails**: 30 actual samples, p50 389 ms, p95 509.35 ms; required p50
is strictly below 300 ms. Reloads take 29.575 / 30.097 / 37.061 seconds,
retain revision 23, deliver the deletion transition from 36 to 19 live leads,
and preserve 11,130,431 source bytes across native restart. These reload
correctness deadlines do not establish critical-boot p95 below five seconds.

The native subphase medians are authentication 6.582 ms, identity stamping
2.996 ms, blocking-pool wait 0.058 ms and full store execution 248.041 ms.
The store can keep working after the browser observes a terminal projection;
these medians are not additive browser stages. The current source separately
measures initial projection and canonical completion/read/projection to locate
that serial tail without relaxing the budget or bypassing authorization.

Browser lifecycle acceptance is also missing: the existing multi-tab browser
test exercises the coordinator and BroadcastChannel handover, not an open
IndexedDB database or shell recovery; the version-floor guard inspects source.
A read-only Welsch observation on 2026-09-08 shows repeated closed-IDB hydrate
failures in shell 0.1.46-beta.29. It is not a reproduction of the close trigger.
Current storage emits a versionchange event when retiring a connection, but
no consumer of that event was found. A controlled lifecycle reproduction must
establish recovery and journal preservation before this gap is accepted.

The current shell's `repairRecoveringDataPlane` restarts affected WebRTC
collections; it neither reopens the local database nor replaces storage
handles. `waitForDataPlaneReady` accepts the presence of a collection, sync
runtime and command bus, which does not prove the IDB handle is usable.
`CtoxIndexedDbStorage.collection` passes its concrete IDB connection into each
collection. Retrying a bridge on those handles cannot repair a closed
connection. The event is emitted by `openDatabase` on version change, but the
Welsch observation does not establish that a version change occurred there.

Required lifecycle acceptance therefore includes two distinct cases: a
compatible database connection lost while writes are pending, and another tab
requesting a newer storage version. In an isolated real browser, retain the
shell and an app subscription, capture the pending journal, provoke each
transition, and verify either confirmed compatible recovery or a visible
incompatibility state. Neither case may delete pending writes, claim healthy
sync on a closed handle, or recover by resetting the browser profile. Measure
the time to recovered reads/subscriptions and confirmed command delivery
separately. These cases have not passed; the coordinator and source guards
must not stand in for them.

Native Sync workflow path filters now also include
`src/core/business_os/**`, `src/core/mission/channels/**` and
`src/apps/business-os/app.js` for both pull requests and main pushes.
Previously, changes only to command completion, queue transactions or the
shell entry point did not trigger this acceptance workflow. YAML parsing and
coverage of seven representative paths in both event filters are checked;
this is workflow configuration validation, not another runtime pass.

No tenant deployment is performed. The sections below retain historical
results and failed attempts; use this status for the current acceptance scope.

## Current browser verification correction

The bundle reproducibility guard previously exited successfully when npx,
the pinned builder download or the build itself failed. The canonical suite
captures successful output, so this internal SKIPPED was reported as PASS.
Builder failure now throws and cleans scratch data; source/bundle drift also
uses the cleanup path. A subprocess regression launches the actual guard with
no builder on PATH: it failed against the old source (exit 0 instead of 1) and
passes after correction. This is a negative verification-path test, not a
completed real bundle rebuild.

Native Sync CI now runs the complete JS/browser suite on Linux with a freshly
built Rust wire daemon. It preserves source/binary provenance and the suite
result without removing any existing app or platform gate. Local full runs
remain red: 119/120 with an inventory timeout, then 117/120 with wire seed,
file ready and multi-tab timeouts. Focused unchanged wire and file checks pass
under Node 24.13.1; the full result must not be inferred from those passes.
Further local broad testing is deferred while the shared resource gate rejects
the overloaded host. No timeout was relaxed.

The four-host artifact for run 34222083233 was read directly: actual built
merge `eb5e060be9d28cb7bf5abccd0dd5cf8089de63ab`, binary SHA256
`faed38976a4c7ef61a2ea33086dfc28d11cd408ccadf075ad73cdc57656d9b47`.
Its unchanged JSON is retained in
[`evidence/ctox-sync-full-host-34222083233.json`](evidence/ctox-sync-full-host-34222083233.json).
One localhost fixture, three voters and one worker: provisioning 5722.821 ms,
reconnect 1017.135 ms, restart 1743.377 ms; 20 warm authority validations have
p50 10.389 ms and p95 11.452 ms. The artifact explicitly records
`codingHarnessExecuted: false` and `businessCommandBudgetEvaluated: false`.

Durability: branch `codex/native-transport-parity`, source commit
`a0e8e59d8f3844ec8f3bfe94b974eec1b8b5093a`, pushed and verified in draft
[PR #69](https://github.com/metric-space-ai/ctox/pull/69).
The existing durable checkout remains in use; no worktree or build cleanup has
been performed while validation is running.

## Finding

At main `8e6f132a82549be9235dd433fc9bd28d5ae868e6`, the three independent Cargo
roots resolved different transports:

| Cargo root | WebRTC/RTC | ICE implementation |
| --- | --- | --- |
| CTOX binary | 0.20.0-alpha.1 | repository patch |
| standalone RxDB tests | 0.20.0-alpha.1 | crates.io, without repository patch |
| standalone Sync tests | 0.20.5 | crates.io |

The RxDB manifest used compatible version requirements. Cargo also ignores
patch declarations in dependencies: the root's ICE patch was absent in both
standalone test manifests. Therefore the historical 74/74 Sync result does not
prove the shipped native transport works. The four-process CLI acceptance
remains red after signaling; parity alone does not establish its root cause.

## Correction

Pin the two direct transport dependencies exactly to the existing production
version and declare the same local ICE patch in each standalone Cargo root.
Seed the standalone lockfiles from the production lockfile and let Cargo
resolve/prune their dependency closures, retaining production versions where
possible. This also avoids a mixture of alpha and stable transitive RTC crates.
The production root lockfile and transport implementation are unchanged.

`native-transport-parity-smoke.mjs`, discovered by the browser runtime suite,
compares versions, registry provenance/checksums and local ICE patch paths for
all 17 WebRTC/RTC packages across the three locks. It also requires exact direct
transport pins. This is a build-input guard, not a networking or feature-parity
acceptance test.

## Evidence so far

- Before correction, the guard exits 1: standalone RxDB resolves registry ICE
  instead of the production local patch.
- After correction, it exits 0 for all 17 packages.
- Cargo metadata resolves offline for aarch64-apple-darwin with the native
  WebRTC feature enabled. Unfiltered offline metadata first failed because an
  uncached Android-only package was requested; no download was attempted.
- Browser-runtime suite: 120 passed, zero failed, zero skipped. This includes
  the new parity guard. The existing wire-daemon fixture was available; this
  run is not a newly built full CTOX host or product-performance acceptance.
- Local production-graph build completed in 50m04s under heavy machine load.
  Unit tests: 19 passed. Authority cluster: 1 passed, 10 failed, mostly on
  confirmation deadlines; recorded RPC delays reached over one second. This is
  a red result, not a production-performance acceptance. Cargo stopped before
  the WebRTC test executable.
- Clean Linux/macOS CI run 34186877185 compiled the production graph. On each
  platform 19 unit tests and 10 of 11 authority-cluster tests passed. The actual
  Workjet IPC consumer test failed because its required sibling checkout was
  absent. No WebRTC result can be inferred from these early exits.

No customer data, runtime configuration, shell slot or tenant release was changed.

## Continuous verification

The existing workflows did not invoke the standalone Sync crate. Add a focused
`Native Sync` workflow on Linux and macOS: compare the 17 transport packages,
check formatting, run the full Sync suite with WebRTC, run native RxDB with the
production ICE patch, and lint all Sync targets. It supplements the existing
app/CLI gates; it does not replace the separate four-process host acceptance.
All runs use locked dependencies and keep the existing test deadlines.

For PR #69 at `3432b3488`, the existing Desktop macOS job stops at npm audit
(`@xmldom/xmldom`, `fast-uri`). The x86_64 Linux CLI job stops before compilation
at the platform-freeze guard (`modules/explorer/index.js` contextmenu handler).
Job IDs: `101935097745`, `101935097780`, run `34186266711`. These unchanged app paths are outside this dependency correction; no guard is bypassed.

Run 34189662536 at `3ac60bffe` exposed two independent failures on Linux and
macOS. The sibling Workjet checkout exists, but pinned revision
`ca7dd885f3615ff31b18eb6ffb6c7c58c45ceaf3` does not contain
`WorkjetSyncIpc.ts`: that file and its generated contracts remain untracked in
the local Workjet checkout. My claim that this pin supplied the actual consumer
was incorrect. The consumer must be reviewed, committed and pinned before this
cross-repository test is reproducible. Its assertion remains enabled and red.

With `--no-fail-fast`, both platforms additionally ran the real WebRTC suite:
4 passed and 10 failed. Peers advertise signaling admission and asymmetric open
channels but cannot confirm execution authority. This reproduces independently
of the overloaded local machine.

## Duplicate local-channel announcement correction

The pinned webrtc 0.20.0-alpha.1 driver invokes `on_data_channel` on locally
created channels too. It supplies a new handle whose event sender is discarded
because the original handle already owns that channel ID. CTOX previously
installed this duplicate, and its immediate event-stream EOF retired the live
connection generation. The newer dependency used by historical standalone
tests fixes that upstream behavior, explaining why those tests hid this defect.

Make registration idempotent for the same data-channel ID within the current
connection generation. Preserve the original event consumer and teardown owner.
Retain the production dependency and ICE patch; do not upgrade the transport
stack or relax authority deadlines as a workaround. The existing failing
native WebRTC scenarios are the negative control. At `4308df4e1`, run
34191792796 reports 13/14 passing on both Linux and macOS (previously 4/14).
The remaining failure, and the one authority_cluster failure on each platform,
are Node ERR_MODULE_NOT_FOUND for the missing Workjet consumer; no native
quorum scenario remains red in this CI comparison.

The missing consumer is now isolated from the dirty Workjet checkout, based on
remote main `ee3a8c92e`, committed and pushed as
`1bdf07370f990aff025374627adc217fc2de2509` in draft
[Workjet PR #32](https://github.com/metric-space-ai/workjet/pull/32).
Its scratch worktree is
`/Volumes/tmp/worktrees/workjet/codex-sync-ipc-consumer`; it remains present
while verification runs. No source changes were made to the dirty canonical
Workjet checkout. CTOX CI now pins this actual consumer revision and also runs
its focused socket tests. Native RxDB and lint continue after a failed Sync
test, provided formatting passed, so one failure cannot hide the other gates.

The first strict comparison found formatting drift in Workjet's generated
TypeScript. The paired generator correction below now resolves this drift.
Full current host execution, browser interoperability, performance and tenant
acceptance remain outstanding.

## Verified paired native run

[Run 34192513718](https://github.com/metric-space-ai/ctox/actions/runs/34192513718)
at CTOX `44947cb1d87562a3d63f63a91480ab124eb526fc` with Workjet
`1bdf07370f990aff025374627adc217fc2de2509` completed successfully on Linux and
macOS: the full native Sync suite, native RxDB suite, Sync clippy and six
Workjet socket tests all passed. Linux logs record 74 Sync tests (including
14 real WebRTC scenarios), 431 RxDB tests and no ignored tests. The WebRTC
executable took 27.07 seconds; this is a suite duration, not a failover or
command-latency percentile.

The final Workjet lint corrections use namespace imports and the Effect test
runtime with HostProcessPlatform. Commit
`1ce94412210fc036db589d96a6d2a8632881ef7e` is pushed to Workjet PR #32.
Its six local socket tests and focused lint pass; local Node was 26.6.0,
whereas CI pins the required 24.13.1. CTOX now pins this final source revision
for a fresh combined run. The previous successful run must not be described
as having tested that newer revision.

At that paired run both PRs remained drafts. Workjet PR #32 was subsequently
merged at 2026-09-08T08:10:30Z after every Workjet CI job passed. Main commit
1a81eabec00fa262c36d72970376f2d09da6a48f has a tree identical to tested
cd0b8f47e511eea9c789c9c6981d25c64f1ab999 (git diff exits 0) and is now the
CTOX CI pin. CTOX PR #69 remains a draft. No tenant upgrade or source cleanup
was performed. The full-host four-process acceptance, current browser/native
acceptance and the specified performance budgets still require proof.


## Generated contract correction and full host gate

The Workjet CI typecheck exposed an actual generator defect: optional fields
in tagged unions were emitted as required TypeScript keys while Effect schemas
already used optionalKey. Both generated outputs now represent the same existing
wire contract. Each schema uses satisfies Schema.Schema<Contract.Type> so
incompatible schema/type output fails consumer typechecking. No cast, relaxed
schema, fixture change or Rust wire change is involved.

Workjet output is formatted in memory using its pinned vite-plus executable,
both when generating and checking. Strict --check remains read-only and compares
all five outputs. The contracts-only TypeScript check, five-output comparison,
host contract shape assertions, generated-file formatting and six local socket
tests pass. Workjet correction commit cd0b8f47e511eea9c789c9c6981d25c64f1ab999 is pushed
in PR #32 and pinned by CTOX CI. Full Workjet CI on that revision is still
required. The scratch worktree is retained; no unique source was deleted.

The subsequent paired native run
[34193660757](https://github.com/metric-space-ai/ctox/actions/runs/34193660757)
passed on Linux and macOS at CTOX 4e0a96160edff1d63a78ad36ef7fe5cc45b7f9bc
and Workjet 1ce94412210fc036db589d96a6d2a8632881ef7e. This confirms the consumer
lint changes, but predates the generated contract correction.

The Native Sync workflow now includes strict cross-repository generation checks
and a separate Linux job that builds the complete CTOX binary, including the
embedded sidecar, then executes host_cli_acceptance with four real host processes.
It retains source revision, binary SHA-256 and the acceptance JSON, not private
fixture stores. Run 34200458404 built the full binary successfully but failed its final revoked-worker
assertion. The acceptance reached admission, validation, reconnect and restart;
no successful acceptance JSON or performance result was emitted. Actual built
Git merge revision: 3f666f36f775288ebc7a39dfd676ae1b7cec52f2; binary SHA-256:
939bc1e86f9a2e258e5d3dfe7bd74544d57f470d0f68b78d46eef1eed2ea9b30.

The voter rejects a revoked worker's validation at the transport admission gate,
before its quorum-confirmed validation handler can return a typed rejection.
The client therefore reports an unconfirmed transport outcome after retrying all
three voters. This is a denial classification defect; no post-revocation
authorization was observed. The four-process assertion remains unchanged.

The focused signed-authority regression failed against the unchanged node at
authority_cluster.rs:1260 with the same admission error (3.15 seconds test time;
10m56s local rebuild under load). The correction permits only a pinned worker's
own status query through admission, then uses the existing quorum and active
membership checks. No additional RPC, string-based error classification or
fallback was added. Proposal and Raft admission stay closed to revoked workers.
The cluster checks now require typed rejection before and after restart, deny
forged identities and another executor's ownership, and ensure an isolated
voter cannot answer definitively from its persisted tombstone. The real WebRTC
check likewise requires Rejected rather than the previous Unavailable. The focused correction then passed locally (1/1, 3.84 seconds), including
quorum loss and restart. The complete native Linux job and four-process host
acceptance also passed on the corrected source as recorded below.

Workjet run 34199994655 is now entirely successful at cd0b8f47e, including Test,
Release Smoke, Mobile Native Static Analysis and Check. Native Linux job
101977812776 also passed the new generation gate, full suites, clippy and socket
tests. macOS job 101977813150 was queued at observation.

The existing host fixture now records twenty warm linearizable authority checks
(all samples plus nearest-rank p50/p95), four-host provisioning, worker reconnect
and worker restart separately. Its topology is localhost with three voters and
one worker, with one job in the control stores. The CI binary is a dev build with
debug information disabled. These measurements do not evaluate the Business OS
command-p50 or collection-boot budgets, WAN behavior, harness portability or
Desktop/Mobile onboarding. Existing deadlines and assertions remain unchanged.

## Successful full host acceptance and measured scope

[Run 34204358510](https://github.com/metric-space-ai/ctox/actions/runs/34204358510)
passed its full-host job 101990269298 with the unchanged four-process assertion.
Source branch commit: 1def1fe35ed1f6d9961df26a340d3e7b15c6a211.
Actual built PR merge revision: 23fee627c6171811a4394bde6af4a51be72d871c.
Binary SHA-256: 71a06e0f5a906c0b0ec4dba7ac5993668b9c077f7b60337438df70a97919be89.
Workjet consumer: merged main 1a81eabec00fa262c36d72970376f2d09da6a48f.

The [unaltered acceptance JSON](evidence/ctox-sync-full-host-34204358510.json)
is preserved in this repository. Original GitHub artifact: 10047656615,
ctox-native-full-host-proof. It proves legacy secret migration, imported
Workjet identity, local listener status and exclusivity, confirmed membership,
worker reconnect, worker restart and revoked-worker denial using four complete
CTOX processes over real localhost WebRTC. No coding harness was executed.

| Measurement | Result |
| --- | ---: |
| Four-host provisioning and initial quorum | 5668.996 ms |
| Warm authority validation p50, 20 samples | 7.675 ms |
| Warm authority validation p95, 20 samples | 8.589 ms |
| Worker reconnect through renewed authorization | 1013.468 ms |
| Worker restart through renewed authorization | 1732.232 ms |

Topology: three voters plus one worker, one job, dev build without debug info,
all on one Linux runner. These are control-plane observations, not Business OS
command or collection-boot acceptance, WAN results, production-load percentiles,
automatic harness failover or UI onboarding.

The new Linux native job 101990269150 passed all its gates. macOS job
101990269060 subsequently failed at 09:22:30 UTC: 13/14 WebRTC scenarios passed,
but voter_reconnect_uses_pinned_key_to_restore_quorum_and_worker_routes timed
out admitting the renewed voter route at webrtc_authority.rs:383. This is an
open native acceptance failure. General CTOX CI at this revision
still stops independently: Desktop Linux job 101990268333 fails npm audit for
xmldom and fast-uri; CLI x86_64 Linux job 101990268429 fails the platform-freeze
guard on modules/explorer/index.js. No guard was skipped and no Office source
was edited. These failures still block a complete CTOX PR acceptance.

## Obsolete remote compiler output removed

On 2026-09-08 at 04:41 UTC, removed only the old isolated test directory
`/home/ctox/.cache/ctox/file-preservation-fbeca32b7/cargo-target` on Welsch.
Allocated size: 28,723,452 KiB. Free filesystem space afterwards: 37,873,111,040
bytes. A privileged /proc audit immediately before removal found no compiler,
open file, working directory, executable or mapped-file reference to that
exact target; no process was unreadable in that audit. The earlier unprivileged
audit was insufficient and did not authorize deletion.

All source snapshots, fixture databases, reports and release-target outside
cargo-target were retained. The removed build was disposable and its correction
and evidence are already in main via PR #65. No service was stopped, restarted
or upgraded; the Office worker owns the Welsch cutover. The remote receipt is
`/home/ctox/.cache/ctox/file-preservation-fbeca32b7/cleanup-native-parity-20260908.json`.

## Closed native connection retained after channel termination

Following the macOS reconnect failure, strengthen the existing generation-race
test to require removal of the current PeerEntry and a fresh connection attempt.
The unchanged implementation fails deterministically with
"a terminated channel must not remain a cached native connection" (one failed
test, 0.05 seconds after compilation). Separately, the unchanged real reconnect
scenario passed once locally in 19.46 seconds; that does not erase the CI failure.

The terminal channel path previously set data_channel_open to false but kept
the PeerEntry and its PeerConnection. Native discovery sees no open route and
retries, but ensure_peer_connection returns that same closed cached connection.
Route initiation therefore cannot make progress after this termination order.

On channel OnClose or event-stream EOF, use the existing generation-checked
full peer teardown. It removes the peer, queues, transfer state and authorization
state, releases blocked senders, closes the transport and emits one disconnect
event. Remove the terminal path's duplicate disconnect emission. Delayed cleanup
from an old generation still cannot remove a replacement.

This closes a demonstrated reconnect defect, but its sufficiency for the
intermittent macOS CI failure remains under validation. The corrected focused
test and browser-runtime suite were still running when this source revision
was prepared. Native WebRTC, full RxDB and clean Linux/macOS CI must pass before
acceptance; no timeout or existing safety assertion was weakened.

## Validation of the terminal-channel correction

At source f1bf5096f792ab1ffccab4c6df920771f01a7912,
[run 34216347377](https://github.com/metric-space-ai/ctox/actions/runs/34216347377)
passed Linux native job 102028880567 and full-host job 102028881007.
macOS job 102028880778 was still queued at observation; its reconnect acceptance
must not be inferred from Linux results.

The full-host job built PR merge revision
4f0b89bb06fc35566f2bc1c22c1c4cc5a38a0127; binary SHA-256:
ea19539f80d8d8b60df73daf57f98589de10964a6dc6e22528c0a18c98252a65.
The [unaltered acceptance JSON](evidence/ctox-sync-full-host-34216347377.json)
proves four complete native host processes, confirmed membership, migrated and
imported keys, listener exclusivity, worker reconnect/restart and revocation.

Measurements on one Linux runner (dev binary, three voters, one worker, one job):
four-host provisioning 5598.458ms; worker reconnect 1017.021ms; worker restart
1732.033ms; twenty warm authority validations p50 8.980ms, p95 10.872ms.
All samples are retained. No coding harness ran, and no Business OS command/boot,
WAN, large-session or UI performance acceptance follows from these numbers.

Locally, the corrected deterministic regression passed (1/1, 1.29s).
The full native RxDB suite then passed 431 tests with zero ignored tests
(394 unit tests, 31 conformance tests and six further integration checks).
It reported an unused remove_peer_presence helper: complete peer removal had
duplicated its operation inline. The shared teardown now calls that helper
with a borrowed peer name; the separate presence tests stay unchanged.
Strict RxDB clippy with all targets and -D warnings passed after using
is_multiple_of in the ACK test fixture; no diagnostic was disabled.

The JavaScript suite returned 119 passed, one failed, zero skipped. The command
consumer inventory subprocess exceeded its unchanged 180-second limit.
Running that same guard separately with the same timeout passed in 26,995ms,
reporting 42 dispatch consumers, 17 projection readers, zero legacy projection
waiters and zero direct intent writers. The overall suite remains unaccepted
until a complete run passes; the isolated pass does not replace that result.
