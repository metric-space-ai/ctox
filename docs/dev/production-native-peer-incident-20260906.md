# Production incident: native peer recovery, 2026-09-06

## Verified native candidate and deployment in progress, 2026-09-07

Main fcce197633914c313b8765454c3afcfe75017d83 preserves complete
module-source content and bounds live replication events through the existing
Resync/byte-bounded pull contract. Historical source documents are excluded
from destructive startup clamping. A real SQLite startup regression preserves
a 2 MiB source document, revision and write timestamp (1 passed).

The verified source artifact covers all 9,846 tracked Git blobs and modes.
Its optimized binary SHA256 is
`705d66d3e487b54f48f5a41cae9f2960e342565300ff96530d8e45e9190b1f26`.
With signed beta17, actual browser/command/native/WebRTC execution preserved
21 source files and 11,130,431 bytes both before and after native restart.
Source command: 341.5 ms; complete transfer and hash verification: 1,160.9 ms;
native restart: 3,170 ms. This is not a production boot-p95 claim.
[Source flow measurements](beweise/raw/module-source-lossless-final-20260907.json).

The final warm-command run completed 30/30 commands: p50 274.5 ms,
p95 341.5 ms, min 214 ms, max 379 ms, with no reported issues.
[Warm-command measurements](beweise/raw/warm-command-final-20260907.json).
The first sequential warm runner failed its port reservation after the source
runner exited; no listener remained. The unchanged workload then used fresh
ports 28987/28986. This setup failure is not hidden as a product pass.
The earlier debug p50 380 ms remains a failed performance run.

The full JS suite subsequently passed 119/119 with the rebuilt wire daemon and
no skips, superseding the intermediate 115/4 timeout result below. The native
RxDB suite passed 430 tests; the source writer and five staging/replay tests
also passed. These do not certify all live application workflows.

The safe managed updater (SHA256 580887fe…) was launched as
`ctox-sync-native-fcce19763-update-20260907.service`. It revalidated the
source tree, test evidence and immutable backups, saved a fresh read-only chunk
manifest, and is rebuilding the managed release before switching. Production
activation and the fourteen source-projection repairs are pending at this
checkpoint. Source backup:
`/home/ctox/.local/state/ctox-incident-lossless-source-20260907`.

Real beta17 browser checks still fail Office opening with
`CTOX product sync push timed out: spreadsheet_blob_chunks`. After an own-tab
reload, lists were empty at a sampled 54 seconds and populated at a sampled
153 seconds. This is sparse observation, not an exact bootstrap percentile.
Several schema imports had previously failed; the exact Support schema URL
subsequently returned HTTP 200 with JavaScript content type. Neither cache
clearing nor the removal of user application data was used to claim recovery.


## Applied staging cleanup and subsequent replay failure

At 2026-09-07T00:37:42Z, maintenance unit
`ctox-sync-office-staging-apply-20260907.service` completed with exit 0.
It backed up all three stores under
`/home/ctox/.local/state/ctox-incident-office-staging-20260907`;
each SQLite quick_check returned ok. The exact candidate digest was
`ae3f82c8d17337c443032d9e5bf004b335a6fcf2263fbcf42ee31165d8564984`.
The audited sixteen canonical DOCX/editor payloads remained intact.

A read-only comparison against that fresh backup covered every stored chunk:
988 desktop, 42 document, 29 spreadsheet records. There were no missing rows;
only the seven intended staging records changed, from active to deleted.
All other complete payload hashes were unchanged. The original staging records
also have a separate durable repair backup.

**This did not yet complete replication acceptance.** After restart the live
projection loop rejected the historical tombstone's `data` object with schema
error 422. The direct writer preserved the invalid `_omitted` object when
deleting; the initial four real-store tests did not exercise the subsequent
schema-validating native replay. A follow-up now sanitizes this invalid byte
field only for deleted file chunks, includes invalid historical tombstones in
the audit, and adds a real native RxDB projection replay test. That follow-up's
build and operational reapplication completed in a second guarded maintenance
window at 2026-09-07T02:01:30Z. Its five real-store/native-replay tests and all
429 RxDB crate tests passed. The exact second candidate digest was
`fd3dad8f973626ad636d219109a5f5337e1152a5c708453b49aec7ccb8af5d17`.
The second store backup is
`/home/ctox/.local/state/ctox-incident-office-staging-v2-20260907`.
After application all seven deleted chunks have string data, and a fresh audit
reports zero candidates, including invalid historical tombstones. Service PID
2717185 was active with NRestarts=0. No validator is weakened and no live
document payload is rewritten.

Six complementary required-field/schema helper tests also passed. The full
JavaScript suite did **not** pass: 115 passed, four failed by timeout
(cross-process wire seeding, demand-cache migration browser, initial-sync stale
browser, multi-tab browser). These remain failures, not production acceptance.
The own browser tab loaded beta16 and Harness after cleanup, but the new
spreadsheet still showed an editor loading skeleton at the next visual check;
no save/reopen success or boot percentile is claimed.

A whole-store recursive marker audit also found fourteen active omitted
`business_module_source_files` records: thirteen
`black-hole-studio:vendor/chunks/chunk-00.mjs` through `chunk-12.mjs`,
and `research:index.js`. Each complete canonical content string exists in
native `business_records` and matches the projected SHA256. These are
recoverable source projections, not abandoned Office uploads; they have not
been deleted or repaired yet.

Signed beta16 is now active (separate rollout, activation
2026-09-07T00:38:02.029726874Z). Only this task's own browser tab was reloaded.
The new test spreadsheet's missing chunk subsequently reached native RxDB;
its editor still failed with `Office RPC timed out: editor.open`.
No save or all-app success is claimed. The service subsequently reported
NRestarts=0 and approximately 6 GB resident memory; the earlier OOM cause
remains unresolved.

## Follow-up, 2026-09-07: font routing, upload failure and guarded cleanup

The independent Office task fixed the gateway classifier for extensionless
signed-slot font files (ctox-dev main
`1cebb53e0cf081fceeec7633f67e17223d31b31b`, promoted deployment
`dpl_FaJ91rpY4RaJoaoez7Lt4TPRPQ31`). Before promotion, the beta15 font
`vendor/ctox-office/upstream/fonts/000` returned HTTP 200 login HTML
(3,877 bytes). Afterwards it returned 708,920 binary bytes with SHA256
`2a6bc04169fc1e273ede5e55ec7c666eb4ebf6da438202cdf61d415a77f83a69`.
Independent fetches took 11.600 s and 7.425 s; these are not a latency pass.
The existing spreadsheet grid then visibly rendered in this task's browser.

Creating one blank test spreadsheet through the UI remains reproducibly broken:
`sheet_4a27209b-66e3-47cf-b147-8a67a2f8230e` (Neue Tabelle 4).
Its record and v1 metadata reached native RxDB, but none of its blob chunks did.
The editor reports `CTOX product sync push timed out: spreadsheet_blob_chunks`,
while the header incorrectly says Gespeichert. Opening another sheet and
reopening this record repeated the failure. No cell edits were made. This
test record still requires cleanup through the supported application path.

The production daemon is still aece8a4f, PID 2694536, NRestarts=2. The
independent task found OOM/SIGKILL entries; current read-only checks confirm
roughly 10 GB RSS / 11.35 GB cgroup memory. This does not by itself establish
the cause of Office saving failures. At 2026-09-07T00:24:32Z the native
command counters showed 263 observations, average observation latency
30,813 ms and maximum 876,605 ms. These cumulative counters are not the
isolated warm-command percentile fixture and do not meet production acceptance.

Seven omitted document chunks were identified as old `office_document_*`
staging uploads belonging to three existing documents. Four versions retain
staged-upload references; their canonical DOCX and editor blob hashes were
independently verified. No canonical blob_id/editor_blob_id points at an
omitted upload. All corresponding persisted commands are terminal.

The new [offline staging repair](office-staging-repair.md) requires an exact
candidate digest, checks all affected canonical history, excludes a running
peer, writes a durable backup, and uses normal projection tombstones. Its four
real-store integration tests passed on the isolated remote source build in
0.20 s (test compilation 9m33s). Production dry-run/application and replicated
tombstone verification are still pending at this checkpoint.

Build attempts are not production evidence: the first lacked the generated
Pi bundle; the second failed linking with SIGBUS and left only 2.3 GB disk
free. Only this task's failed incremental compiler cache was removed,
restoring 20 GB. The third build uses no incremental cache and passed the
four repair tests. No production database was deleted or overwritten.

## Subsequent live follow-up: beta15 and remaining UI latency

A separate Office rollout activated signed beta15 at
2026-09-06T22:16:10.982981761Z, keeping native aece8a4f unchanged.
Read-only shell status confirmed active beta15, phase=current, health=healthy,
administrable=true, recoveryShell=false. After navigating only this task's
test tab, a screenshot confirmed beta15 and the rendered CTOX Harness flow,
task list, timeline and token metrics.

This is not an all-app acceptance: Browser automation encountered CDP
dispatch/Runtime.evaluate timeouts while opening apps. A CTOX navigation
that reported a timeout subsequently appeared completed in the screenshot.
An earlier unscoped close selector was also ambiguous across six windows;
no successful close was claimed. The Tickets navigation succeeded but showed
a continuing sync message. Read-only native inspection found all twelve
ticket projection collections empty, so zero visible tickets alone is not
evidence of lost records.

The operator machine simultaneously showed a Codex renderer at 119.8% CPU
and several other busy applications/build processes. That snapshot does not
identify the test tab's process or establish the cause of the delay.
End-to-end UI responsiveness, cold boot, and the Office flows therefore remain
open. No process was killed, store changed, or timeout weakened to claim a pass.

## Production rollout completed: native aece8a4f and signed beta14

Current checkpoint: 2026-09-06 22:14 UTC. The authentication failure and
not-yet-deployed statements below describe earlier phases and are superseded
by this section.

All runtime fixes were merged and pushed to main at
`aece8a4f28999885b4fe788f2c3bf4559a4bcf38`. Signed release
`business-os-shell-v0.1.46-beta.14` was built from that exact commit by
GitHub Actions run `34059620173`. The managed source archive was independently
checked against all 9,820 Git blobs and executable modes.

The safe managed updater completed successfully (exit 0) in 26m26s:
`ctox-sync-welsch-native-aece8a4f-update-20260906.service`,
invocation `6526028848594260993f923a39c46614`.
Welsch now runs `branch-main-aece8a4f-20260906`; the deployed binary SHA256 is
`d2832ec31e002a8fec46390f68384921e34f8d6c6ad38bedc22505e3eb2f2e9e`.
This managed rebuild is distinct from the isolated optimized test binary
`580887fe…`; do not conflate their binary hashes or performance results.
The previous `branch-main-490f1ab80-20260906` release is retained.

Beta14 activation and the service restart succeeded through
`ctox-sync-welsch-beta14-activate-20260907.service`,
invocation `6bb49d2265e14b53b16dd54748389d35`.
At the checkpoint, shell status reported active beta14, healthy, administrable,
and no recovery shell. A separately prepared beta15 was desired/ready; this
report does not claim to have activated that subsequent release.
Native peer status reported running=true, replicationUp=true, fresh heartbeat,
and errorTotal=0.

A fresh SQLite backup of all three stores passed quick_check before this
update. The postflight at 22:13:58 UTC compared all 1,059 baseline chunk payloads
under a read-only transaction: 988 desktop, 42 document, 29 spreadsheet.
Every payload hash and deletion state was unchanged; none disappeared or became
newly omitted. Seven historically omitted document chunks remain unresolved.
This verifies preservation during this rollout, not recovery of those bytes.

In the real signed-in browser test tab, the shell visibly displayed beta14.
Opening CTOX rendered the Harness task list, flow diagram, timeline, and token
metrics. Existing tasks still displayed error states. This confirms restored
rendering, not successful new task execution. The user's original tab was
preserved. Spreadsheet loading/reopening remains a separate unresolved flow
at this checkpoint; the next scoped fix is handled separately.

The final isolated signed-beta14 Browser/WebRTC/native/browser test completed
30 commands with p50 **250 ms**, p95 **290.55 ms**, min 206 ms, max 292 ms.
No missing assets, request failures, cache repair or startup reload occurred.
Unit `ctox-sync-shell-beta14-browser-20260906.service`, invocation
`b2d4c0a5fd124a0f887f7553d065ca67`, exit 0.
The warm fixture passes. Critical-collection boot p95, managed-production
command latency, fleet-wide instances, desktop/mobile, and complete Office
editing/reopening are not certified by this result.

Evidence:
- `beweise/raw/shell-native-beta14-roundtrip-marks.json`
- `beweise/raw/shell-native-beta14-stage-report.json`
- `beweise/raw/main-shell-aece8a4f-source-report.json`
- `beweise/raw/welsch-shell-beta14-pre-backup.json`
- `beweise/raw/welsch-beta14-postflight.json`

The shell artifact package check now also runs the actual-helper browser asset
routing regression (commit `04affc635`); 16 artifact checks and all six browser
cases passed. No production behavior was changed by that CI wiring.

## Final root-path and app-asset regressions

The follow-up fixes `/business-os/`: its empty relative path previously skipped
signed document pinning although `/index.html` passed the browser fixture.
All four entry paths now share the document resolver. The final native server
source SHA256 is
`e99626e47c35883ae46c30516986bec8f88b1dc688583d80525342d916a8e5ba`.
The optimized binary SHA256 is
`580887fe5133089d77dc6dd1a44c0e865e539c9a1d22285738ce6ee8d602790c`.

The native suite again passed 45 tests. The first HTTP probe exceeded its
five-second request deadline under a one-CPU build unit; the build itself
completed successfully in 10m47s. A separate two-CPU fixture retained the
five-second latency assertion while allowing longer transport observation:
all 12 requests passed, including same-size corruption and removal after
admission, restoration, absent release (410), invalid address (400), and absent
inventory entry (404). The first root request took 3,467.6 ms; subsequent entry
documents took 28.3–78.2 ms. This is one run, not a boot p95.
Report: `beweise/raw/shell-native-final-http-probe-report.json`.

The canonical Browser/WebRTC/native/browser fixture completed all thirty
commands with p50 **250 ms**, p95 **339.75 ms**, no missing assets, request
failures, cache repairs or startup reloads. Unit
`ctox-sync-native-shell-final-flow-20260906.service`, invocation
`20fba3ca69ae4dcb8c52a65ec0ac0ebe`. Its navigation phase nevertheless took
**20,297 ms**. Warm command acceptance passes for this isolated fixture;
cold boot and production performance acceptance remain open.
Reports: `beweise/raw/shell-native-final-roundtrip-marks.json` and
`beweise/raw/shell-native-final-stage-report.json`.

The shell now resolves installed and local module scripts, frames, icons and
styles outside the immutable slot. Stylesheet reuse/revision replacement compares
normalized URL paths, avoiding duplicate styles on warm mounts.
`src/apps/business-os/scripts/shell-asset-routing.browser.mjs` uses the actual
production helper bodies in a browser component fixture: six cases cover the
three module sources under both source and pinned shell URLs with real HTTP
imports/fetches/images/styles. It does not claim full app or Sync acceptance.
The static Shell-V2 contract passed 37/37 apps and geometry passed CTOX,
Documents and Spreadsheets at 1180 and 720 pixels, six cases total.

The candidate has not been activated on production. GitHub workflow dispatch
returned HTTP 401 and the release-tag push could not obtain HTTPS credentials.
The earlier main push through `832e5058e` succeeded. The later root/app fixes
must also reach origin/main before the next signed shell is published.
The unpublished local beta14 tag for the earlier source was removed; the next
release tag must name the complete, verified main revision. Production remains on native
`branch-main-490f1ab80-20260906` and signed beta13 while maintenance/readiness
and real browser app operation remain unresolved.

## Final native checks and deployed data preservation

The final native source also removes the silent archived-tree fallback and
propagates a selected slot's verification failure. Its server source SHA256 is
`5f10a2b1ae9de07c78088df8d3831a87b851c164bee1e359c5344f532a2e5332`.
The isolated `native-shell-final-tests.log` records **45 passing native tests**,
including three real-store resolver regressions, all signed-slot tests, credential
tuple checks, file preservation, safe updater recovery and Pi-sidecar tests.
The earlier 30-command browser result below predates this final resolver change;
a final optimized browser run remains required.

The complete JS suite passed **119/119, zero skipped**, using the real wire
daemon and Playwright browser binaries. The first attempt's eight failures
were missing rustfmt/browser executable paths; the corrected full run is
`ctox-sync-native-shell-js-suite-v2-20260906.service`,
invocation `97cdb4452e3b42829dab99385830ff64`.

Production switched to `branch-main-490f1ab80-20260906` before the shell change.
At 18:53:56 UTC a read-only SQLite snapshot comparison against
`ctox-incident-pre-native-20260906T162646Z/file-chunk-manifest.json` found all
**1,057 baseline payloads unchanged** (988 desktop, 40 document, 29 spreadsheet).
No baseline row disappeared, became tombstoned or was newly omitted.
Seven historical document payloads were already omitted and remain unresolved.
This verifies preservation during that update; it does not restore those bytes
or certify browser editing. A fresh production browser still showed an empty
workspace while the maintenance/readiness path was under separate diagnosis.

## Verified signed-slot before/after comparison

The corrected runners validate the complete CTOX test-root markers and assert
`currentSlot` in that exact root's database before launching the native server.

- Old control: `ctox-sync-browser-timing-old-control-20260906.service`,
  invocation `bfadee8ee9de4d5cb1487c6473e0138e`, verified
  `0.1.46-beta.13` active in `browser-timing-old-control-20260906`.
  The server reported serving that signed slot, then returned HTTP 500 for
  `command-bus.js` and `sync-contract.js`; the browser fixture exited 1.
- Candidate: `ctox-sync-browser-timing-candidate-v2-20260906.service`,
  invocation `0d506454c65a4e838154bfbf29533bda`, the same signed release in
  `browser-timing-candidate-v2-20260906`. The fixture exited 0 after thirty
  complete Browser/WebRTC/native/browser commands, with no missing assets,
  failed requests, asset response errors or cache repairs.

The candidate native binary SHA256 is
`f530560fd2b5d883197f9cf7c47a07f707f862ee421f76a9102d415cf93e7c15`.
Its source patch from the recorded f70 baseline has SHA256
`55a8a587bb1d9894bd37f053674725caf5eb891e262ec6ba0f55eb6579d60c3c`.
It is an isolated unoptimized development build, not a production release.
Its measured end-to-end p50 was **322.5 ms**, p95 **371.75 ms**: the functional
flow passes, but the under-300-ms p50 acceptance is not met. The optimized
release and deployed instance still require their own measurements.
Raw seven-mark samples and the complete stage report are retained at
`beweise/raw/shell-native-candidate-debug-roundtrip-marks.json` and
`beweise/raw/shell-native-candidate-debug-stage-report.json`.
Cross-process corrected stage estimates are diagnostic; the total above uses
the browser's own start and terminal-observation timestamps.

## Fresh-browser shell boot regression

The unchanged canonical browser/native command fixture was run twice against
the running release's exact native executable `branch-main-20260906T120734Z/bin/ctox-real`,
with isolated roots, synthetic stores, loopback signaling and a fresh pinned
Chromium headless shell (Playwright 1.60.0). Neither run reached command timing:

- `ctox-sync-browser-timing-baseline-20260906.service`: raw main-derived shell
  source; failed during boot.
- `ctox-sync-browser-timing-signed-20260906.service`: attempted a signed-slot
  test, but the CLI rejected the incomplete test-root shape and silently used
  the isolated source checkout instead. The browser server then used its own
  separate root without that active slot. This run did not test signed-slot
  delivery. The first candidate browser run repeated this setup mistake.

The native server returned HTTP 500 for `shared/command-bus.js` and
`shared/sync-contract.js` with the internal v338 revision, reporting
`active Business OS index.html does not declare a shell generation`.
The entry document uses `20260906-office-page-exit`, while the native resolver
recognizes only tokens containing `-shell-v2-`. Both observed failures
therefore prove the source/recovery-tree boot regression, not a signed-slot
regression. Read-only inspection confirmed that the source test root was
activated at 18:15:44 UTC, while production retained its original 15:38:52 UTC
activation. The corrected runner creates all required root markers and asserts
the active slot in the exact fixture database before starting the browser.

An isolated candidate now addresses signed shell files by immutable release
paths and pins the document base to that admitted release. It preserves a
separate path for runtime-installed apps. This candidate is **not deployed or
accepted**. Its first full `cargo check --tests` was killed at the explicit
3 GiB memory limit; that is not a passing compiler result. A previous check
started before the source transfer completed and was stopped; it is also not
candidate evidence. The subsequent run verified all nine source hashes first.
The existing static Shell V2 guard passed for 37 apps; the real-browser geometry
lab passed CTOX, Documents and Spreadsheets at 1180 and 720 pixels (six cases).
Those layout checks do not prove bootstrap, persistence or performance.

## Combined candidate regression and native latency diagnosis

The safety candidate in
/home/ctox/.cache/ctox/office-file-preservation-validation.o18IsS passed
two automatic-recovery tests and sixteen Pi tests, but its file-preservation
test failed at 17:07:17 UTC with an omission marker replacing 327,682 bytes.

The candidate's rxdb_peer.rs and rxdb_peer_demand_files.rs hashes still match
the previously passing source. Its store.rs instead hashes to
9881b8442f499475317dea34f436f00273cce059d99e005979d9a382bc1df48f.
The reviewed diff adds the intended Pi source conflict checks but also removes
RxdbCollectionWriter.demand_file_storage, its constructor assignment and three
forwarding arguments, and restores the unconditional payload clamp. Those
removals undo the main file-preservation fix. The failed combined candidate
is not covered by the earlier passing result. No activation by this task
followed that failure.

A bounded native-only status-command diagnostic has raw evidence at
beweise/raw/native-ipc-welsch-20260906.json. It used the existing authenticated
command path and timing probe on the old running native release, during
concurrent isolated compilation. Twenty-three complete timing samples yielded
IPC p50 1,931.6 ms / p95 2,692.8 ms, handler p50 1,272 ms and projection
p50 8 ms. The batch deadline interrupted the next client wait; its durable
command ID was subsequently confirmed completed without replay. This is an
incomplete diagnostic batch, not a thirty-sample browser/WebRTC or local
fixture acceptance.

The existing full browser fixture is
src/core/rxdb/tools/browser_rust_smoke.js with
SMOKE_MODE=command-roundtrip-timing-browser-to-rust and SMOKE_PAGE_PATH=/index.html.
It already emits thirty samples with seven timing marks and a stage report.
Its pinned Playwright 1.60.0 dependency and Chromium headless shell have been
prepared in the owned isolated test root. No production browser profile is
used. The test source was advanced from the immutable fbeca32b7 archive to
main f70b1b5a7 via an exact Git patch with SHA256
37b004367c6cdb9bcea47db72427d1efc995c3e5c1fe6427b0e56218cf39b65f.

## Native file-preservation test result

The release-mode retry completed successfully at 16:22:28 UTC. Its journal
records exactly one test passed, zero failed/ignored, 3,201 filtered, in
0.02 seconds after a 7m20s build. It exercised the real root test binary
/home/ctox/.cache/ctox/build-office-20260906/release/deps/ctox-b76f227b4caead72.
The three changed native files in the tested main576 source exactly match the
immutable fbeca32b7 archive by SHA256:
- rxdb_peer.rs: 16711dbc104109229ab66dbbbe1f63ad3a8a611da0e74d6aff202d0c452fbb41
- rxdb_peer_demand_files.rs: c3d1040367ff7af4b3ec0d8fd5acf93aed8dd50eda1371c6490b263fd9a861e7
- store.rs: 549054acffb3e6ed3a2c922fd62f9cc421e5e548500a31951dca813ecce8fd06
The peak was 8.3 GiB; this explains why the separate 6 GiB attempt could not
finish compilation. The successful assertion is a file-store restart test,
not production activation or the complete user-story/performance matrix.

## Beta13 read checks and repeated Word cache recovery

On the current beta13 shell, the real authenticated browser again showed
the CTOX Live Flow, five tasks, the flow diagram and token counts. The beta9
CSV acceptance file painted its saved rows (12/42, 7/8, 19/50). XLSX X2 painted
its saved edit marker and preserved unrelated values. These are actual
read/reopen observations, not a new edit/save or full performance acceptance.

The native peer reported running=true, replicationUp=true, fresh heartbeat,
health.errorTotal=0. The last startup projection durations remained high:
business_records 15,383 ms, knowledge_tables 21,237 ms and desktop_file_index
7,322 ms. Command observation had only two samples, mean 7,503 ms and maximum
9,704 ms; these are not the specified local-fixture p50. A browser screenshot
taken 30 seconds after CSV selection and 48 seconds after XLSX selection
confirmed rendering, but those sampling delays are not measured paint latency.

A subsequent read-only audit found seven omitted document chunk rows, zero
omitted spreadsheet chunk rows, and one currently selected document version
still pointing its editor_blob_id at a damaged chunk. The original three
regenerated editor pointers remained repaired; historical staged pointers
still name damaged artifacts. The newly affected current Word version was
doc_0d4889d0-fe96-453f-85b7-9f01a2f62eae_office_v3_RDwUEoRItd.
Its canonical DOCX passed SHA256 and ZIP CRC verification: 1,188 bytes,
52a6477fb766d1d4f54790edba8c7f34a7e0808ccd966c5dd397e1503200fc5f.

Fresh SQLite backup API copies of both stores passed quick_check at
/home/ctox/.local/state/ctox-incident-file-recovery-20260906T160847Z/.
The authenticated command
cmd_incident_20260906_reprepare_doc_0d4889d0_v3_authorized completed using
the existing maintenance principal and unchanged permissions. The token
remained in the subprocess pipe. No direct SQL repair was used. The real beta13 browser subsequently painted the saved, bold Word text from that current version.

The separate release-mode native regression initially failed compilation
because its required pi-sidecar bundle was absent. This was another build
failure, not a failed assertion or passing test. Native activation remains
unverified. The destructive native startup clamp is still in the running
release; successful cache regeneration does not remove that root cause.

A retry of the original immutable fbeca32b7 source is now isolated in
ctox-file-preservation-debug0-fbeca32b7.service, invocation
c52d5317229540a9b595e1e89d0681cb, using test-debug0.sh/test-debug0.log under
the original test root. It reuses the built sidecar and Cargo dependencies,
sets only profile.test.package.ctox.debug=0, removes the virtual-address limit,
and retains MemoryMax=6 GiB, one CPU, Nice=10, one Cargo job and a one-hour
runtime bound. The test assertions and source are unchanged. The separate
release-mode unit was confirmed failed/inactive before this retry started. The debug0 retry subsequently hit its physical MemoryMax limit (systemd Result=oom-kill, ExecMainStatus=15), before any test result. No higher-limit duplicate was started: production ctox-real alone held approximately 10.4 GiB RSS (10.3 GiB anonymous) and another release-mode test was using several GiB. Both aborted own attempts remain non-passing verification.

Status: partial recovery. Welsch boots; the real beta13 browser renders the
Harness flow and the checked Word, CSV and XLSX contents. The native storage
regression passes, but its production activation remains outstanding.
Full edit/save/restart E2E, historical artifact recovery and performance
acceptance remain open. A separate fresh pre-activation backup at
/home/ctox/.local/state/ctox-incident-pre-native-20260906T162646Z/ passed
quick_check for both stores. Its file-chunk-manifest.json records data hashes
for 1,057 chunks across desktop files, Word and spreadsheets, including the
seven previously omitted Word chunks. This provides a comparison baseline;
it is not a claim that activation or post-activation verification occurred.

## Routing mitigation already active

The production alias `ctox.dev` and wildcard `*.ctox.dev` resolve to Vercel
deployment `dpl_7Cvc8DeqVjkokApXfEYTzbkjpfP2`
(`ctox-nlzd986z9-metric-spaces-projects.vercel.app`, READY).
The premature strict generation-conflict rollout remains reverted by ctox.dev
main commit `63eb4444577f88842011033c1c66924a849a3dd6`.
Do not remove the compatibility fallback again before the complete native and
client dependency-generation contract passes real upgrade/browser tests.

## A subsequent independent native upgrade failed after reporting success

Welsch's pre-existing systemd job
`ctox-office-native-upgrade-20260906.service` began at 10:29:22 UTC.
It compiled and activated `branch-main-20260906T102922Z`, then reported
`updated: true` and completion at 10:57:00 UTC after 27m37s.

At 10:57:03 the service panicked inside `schema_from_json`:

> invalid type: sequence, expected a string

The panic was at `src/core/business_os/rxdb_peer.rs:8933`.
The process and systemd unit remained alive, while its native RxDB peer was
dead: running=false, replicationUp=false, heartbeatFresh=false, errorTotal=1.
The browser consequently retained maintenance write protection. This disproves
the updater's process-running-only success criterion.

The relevant Rust fragment models JsonSchema.type as Option<String>, while
JSON Schema permits type unions represented as arrays. The failing packaged
field was subsequently isolated as `ctox_crew_members.active_task_id`, whose
contract is `["string", "null"]`. An independent regression using every packaged
Business OS schema reproduced the panic before the fix. Do not normalize away
the union or weaken validation merely to suppress the panic.

Main commit `01fe390c7` adds native union representation and validation. Its
crate-level success did not establish daemon compatibility: Welsch's subsequent
`ctox-office-native-union-upgrade-20260906.service` build failed with three E0599
errors at the remaining `Option<JsonSchemaType>::as_deref` consumers in the
Business OS projection layer. The build failure prevented activation of that
candidate. Its recovery script restored the prior service executable without
restoring database snapshots. Projection normalization and repair/tombstone
defaults must explicitly handle unions, and the entire CTOX binary must pass
compilation before another activation. Native-crate tests alone miss this boundary.

## Data-preserving emergency service fallback

The standard `ctox update rollback` was deliberately not run: its implementation
restores the complete update backup, which would rewind data to before the
27-minute build.

Instead, only Welsch's service executable was temporarily switched back to:
`/home/ctox/.local/lib/ctox/releases/branch-main-20260905T193754Z/bin/ctox-real`.

The explicit systemd drop-in is:
`/home/ctox/.config/systemd/user/ctox.service.d/90-incident-native-recovery.conf`.
It clears ExecStart and starts that binary with `service --foreground`.
The current symlink, runtime data, active shell slot, and install manifest were
not rewound. The manifest therefore names the newly installed release while
the service executable is temporarily older. This is an incident mitigation,
not the final runtime compatibility contract. Preserve the previous release
until this drop-in is removed after a verified forward repair.

Backup location on Welsch:
`/home/ctox/.local/state/ctox-incident-20260906T1105/`.

- `current-state.tar`: state archive excluding the existing backups directory
  and sockets. First attempt returned tar exit 1 because SQLite sidecars
  disappeared and the root directory changed. It must not be represented as a
  self-contained valid SQLite snapshot.
- `sqlite-snapshots/`: 42 databases copied with SQLite's backup API, each
  passing PRAGMA quick_check; manifest lists the databases.
- Restore planning must use those SQLite snapshots in place of database files
  and WAL/SHM sidecars from the tar. No restoration was performed.
- `current-state.tar.sha256` and `ctox.service.before.txt` retain archive
  integrity and the original service definition.

The original service was restarted automatically when the first backup attempt
failed. Only after the database snapshots passed did the executable change.
Service start: 11:06:58 UTC, PID 2429984.
Native replication startup log: 11:07:05 UTC, 198 collections registered.
Subsequent projected health: running=true, replicationUp=true,
heartbeatFresh=true, errorTotal=0.

The temporary fallback also logged a schema-hash conflict for
`workjet_computers`. It is not acceptable as a permanent mixed-version runtime.

## Browser and fleet observations

All actions used existing operator access; no other Codex task was messaged.

- Welsch fresh page: shell 0.1.46-beta.8, signed document source
  87daa431b1604bfcca364a1eb1a851e90da1874d. Desktop boots. After peer recovery
  the maintenance banner disappeared and the CTOX harness container appears.
  A later screenshot also verified the painted Harness flow, task list,
  progress and timeline. This does not certify execution of those tasks.
- Documents still reported document_versions peer-reopen timeout.
- Spreadsheets still reported Office RPC editor.open timeout.
- SKF: public page reaches the desktop and research data; native replication
  becomes active while its browser is connected. Shell is explicitly Recovery.
- Thesen: public page reaches the desktop and app surfaces; shell is Recovery.
- Miltonticket: public route reaches its login page. No credentials were
  entered; authenticated E2E was not performed.
- All four native peers had fresh heartbeats and zero health errors in the
  final fleet observation. A disconnected browser can leave replicationUp
  false; this flag is not equivalent to a dead daemon.

## Measurement limits and next acceptance

The native peer registered replication 7 seconds after service start.
This is not a critical-collection browser-boot p95, warm command p50, or a
full user-workflow latency measurement. No performance acceptance is claimed.
No production-readiness claim follows from these incident checks.

Required forward repair: preserve type-union schema semantics, certify every
actual schema through the native parser and real persisted collections,
correct the updater to reject a dead mandatory peer, resolve the
workjet_computers schema conflict through the approved migration path, remove
the temporary service override, and test actual Documents/Spreadsheets/Harness
flows with persistence and timings. The original shell-generation work remains
unfinished and must not be deployed during this recovery.

## Follow-up at 12:18 UTC

The daemon union integration is on main in `3fffd9f0a`; merge `3a4d45bf7`
preserves the independent full-property-contract regression and additional
projection coverage. The unfinished shell-generation draft is not included.

The first union forward build failed on three remaining daemon consumers.
The subsequent `ctox-office-native-projection-upgrade-20260906.service` is
still running (PID 2451497). Welsch remains on the recovered service
(PID 2449549); its peer reports running=true, replicationUp=true,
heartbeatFresh=true and errorTotal=0. This is not evidence that the corrected
release has been activated. Browser update maintenance still blocks app writes
during the build. Harness task list, painted flow, progress and timeline were
verified again; Documents and Spreadsheets are not accepted.

All four public entrypoints returned HTTP 200 in three requests each.
[Raw entrypoint timings](beweise/raw/production-entrypoint-timings-20260906.json)
record TTFB from 0.266132 to 2.943577 seconds. These unauthenticated HTTP
measurements exclude WebRTC bootstrap, critical-collection readiness and
authenticated command/app latency. They are not a p50/p95 acceptance.

Local full-daemon checking, the expanded native schema regression and the JS
suite were still running at this checkpoint. No new pass result or production
readiness is claimed from these pending commands.

## Forward repair and independent browser check after 13:00 UTC

The corrected native release `branch-main-20260906T120734Z` is active.
The emergency executable override was removed after the forward activation.
A subsequent read-only check observed PID 2460544, running=true,
replicationUp=true, heartbeatFresh=true and errorTotal=0. SKF, Thesen and
Miltonticket also reported running peers with fresh heartbeats and no health
errors; their replicationUp=false without connected browsers is not an outage.

A fresh authenticated Welsch page now identifies shell 0.1.46-beta.9.
The original startup failure did not recur. An existing Word acceptance
document painted its persisted text, and the CTOX task list, flow, progress
and timeline rendered. No new task execution or document-save acceptance
is implied by these read-only checks.

Spreadsheets remains a failing user story: both the beta9 CSV acceptance
record and an existing XLSX display "no saved version". The browser log
instead records repeated WebRTC peer-reopen timeouts for spreadsheet_versions.
Read-only SQLite inspection confirms that the CSV record's current_version_id
points to an existing, non-deleted version. The UI must not turn a transport
failure into an assertion that the user's saved file is missing.

Two additional shell sync defects were reproduced independently:

- The local tab coordinator used APP_BUILD alone. beta8 and beta9 share v340
  despite different canonical DB bundles, allowing a corrected tab to follow
  an older runtime. The coordinator key now includes RXDB_BUNDLE_URL from the
  sole canonical loader, as well as the shell epoch.
- In a follower tab, an ordinary startCollection overwrote an already acquired
  direct bridge with a follower stub. The next forceDirect acquisition could
  replace its still-active registration. Cached live bridges are now reused
  before the follower branch. Actual role transitions still own demotion.

Both regressions failed before the corresponding correction and pass after
it. The acquisition test runs the real multi-tab coordinator and shell runtime
with a simulated native transport; it is not production WebRTC E2E. The
37-app static shell contract and all 37 geometry cases also passed.

The coordinated source build identifier is now
`20260906-shell-v2-sync-bridge-v341` in HTML, APP_BUILD, DB and sync loader
imports. An actual public sync.js response at 13:18 UTC carried
`cache-control: max-age=14400, must-revalidate`; reusing v340 would leave
previously cached code eligible for four hours. This source fix is not an
activation or production-readiness claim.

Browser automation observed a warm Harness switch plus accessibility
inspection in 23,012 ms and a Tables switch plus inspection in 7,951 ms.
These wall-clock observations include automation/inspection overhead and
must not be reported as app interaction latency, command p50 or boot p95.
The performance acceptance remains open.

A separate read-only data audit found three non-deleted document chunk rows
containing omission markers (324,210 / 324,326 / 324,318 original encoded
bytes). Each has a document reference but no live document-version reference.
Their originals were not found in the two examined native record/chunk
stores. This audit checked canonical blob_id references only and was
incomplete: editor_blob_id and staged_editor_blob_id must also be checked.
The later restart investigation below confirms damage to a referenced editor
cache. The earlier lack of canonical references is not evidence of safety.

## beta10 activation and referenced file corruption

Commit 87abdd514 is on main. Its signed shell release 0.1.46-beta.10 passed
GitHub Actions run 34036560364 and was activated on Welsch at 13:38:26 UTC.
The service was restarted by this task (PID 2479926); the browser loaded
v341 and beta10. The native executable stayed branch-main-20260906T120734Z.
No database backup was restored.

The native peer reported running=true, replicationUp=true, heartbeatFresh=true
and errorTotal=0, but real Office workflows still failed. At 13:41:36 UTC the
browser reported ctox_webrtc_incoming_transfer_stalled across collections;
Spreadsheet editor.open subsequently timed out. Word reported an atob error.
These health flags therefore do not establish app readiness.

The restart logged that it trimmed 201 oversized projected documents.
The startup clamp scans all physical Business OS collection tables, including
file chunks. Browser Office writes chunks up to 256,000 bytes; Base64 encoding
can exceed the 262,144-byte projection budget. The generic clamp replaces
data with an omission marker and increments its revision. Direct native
projection writes have the same lossy clamp.

For document doc_0d4889d0-fe96-453f-85b7-9f01a2f62eae, current version
doc_0d4889d0-fe96-453f-85b7-9f01a2f62eae_office_v2_-z9gChy2PR references
editor blob office_document_66f73256-6e61-49eb-bebc-6a7e6a172488.
Its first chunk contains an omission marker for 324,442 encoded bytes.
The canonical DOCX remains intact: 1,163 decoded bytes, matching source_sha256,
valid ZIP CRC, and word/document.xml containing the previously observed
Office-Abnahme text. This verifies this canonical file, not every file or
every formatting property.

Recovery command cmd_incident_20260906_reprepare_doc_0d4889d0 was dispatched
through the daemon-owned office.document.prepare command. It failed with
role_or_scope_denied (data.write, documents), so no regenerated cache is
claimed from that rejected attempt. No direct SQL repair or permission bypass
was performed.

At 14:33 UTC, SQLite backup API snapshots of business-os.sqlite3 (129,073,152
bytes) and business-os-rxdb.sqlite3 (217,681,920 bytes) both passed quick_check:
`/home/ctox/.local/state/ctox-incident-file-recovery-20260906T143338Z/`.
The existing codex-shell-rollout maintenance identity then authenticated via
the supported local issue-capability command, without creating users or
changing roles/grants. The token stayed in the subprocess pipe and the
daemon's normal policy gate accepted the authenticated prepare command.
`cmd_incident_20260906_reprepare_doc_0d4889d0_authorized` completed.
The canonical source hash remained
214c1da6cdbbb67a7d8e6ffb2b8c68b356deb3a26828d934f4f3ee3560aeb811.
The new editor payload passed native protocol validation; reopening the
document in the real beta10 browser painted its saved Office-Abnahme text.
This is a successful read/recovery check, not edit/save or performance acceptance.

The same audit found two more current document editor caches with omission
markers and verified canonical hashes: doc_0d1190f0-d6e1-482a-844c-754331fcf528
and doc_25e4549b-c56a-4660-a829-8dab4805cf76. Their authenticated prepare
commands also completed. The W3 document subsequently painted its saved
"Greppy browser readiness W3" text in the real browser. The third document's
rendering was not confirmed before the dedicated test tab was closed.
Historical staged_editor_blob_id references still retain the damaged artifacts;
regenerating a current editor cache does not recover every historical artifact.
Three older spreadsheets had no canonical chunk rows in the inspected RxDB
table. Other stores/backups must be checked before classifying them as lost.

The pending native correction excludes built-in and runtime-declared
demand-chunks storage collections from both write-time and startup projection
clamping, using the canonical demand-file registry. It does not exempt
desktop_files metadata or weaken unrelated projection budgets. Verification,
native activation, remaining damaged-artifact recovery and full Office E2E
remain open. The local build machine was severely overloaded during verification
(load average 120.01, 0.49% CPU idle, 13 GiB compressed memory at 14:43 UTC);
browser automation wall times on this machine cannot serve as isolated
production-performance acceptance.

The correction is on main in 88b4af3f8, merged with the subsequent retired-
transfer and complete runtime-import-chain corrections in 812dd5a5d.
The merged data-plane guard and retired-transfer boundary checks pass.
CI run 34040775617 passed the Linux ARM native compile check. The aggregate
run is not green: the Linux x86 job stops at the Explorer module-local
contextmenu freeze guard; Desktop dependency audits stop at xmldom/fast-uri
advisories. No guard or audit was weakened.

The root SQLite preservation regression was still compiling on the overloaded
local host. Its first invocation started before the built-in registry lookup
optimization and the main merge; it cannot certify the final tree on its own.
Adding the regression as an explicit Linux ARM CI execution step was rejected
by GitHub because the available OAuth credential lacks workflow scope. That
unpublished workflow change was withdrawn; the existing CI remains unchanged.
The native preservation regression still needs a completed local run against
the final source. No native rollout has followed this source change.

Latest public route probes returned HTTP 200 for Welsch, SKF, Thesen and
Miltonticket (individual TTFB 0.358, 0.313, 0.952 and 0.909 seconds respectively).
These remain route probes, not authenticated app or performance acceptance.
The beta9 CSV acceptance file still failed with Office RPC editor.open timeout
in the real beta10 browser.

## Isolated final-source native verification in progress

The local test invocation was deliberately cancelled through its supervisor
(exit 143 / child -15), after the final-source server run started. It did not
execute the SQLite regression and is not a passing verification result.

An immutable archive of main commit fbeca32b7 was uploaded and verified:
SHA256 1d5762e5036b900a947994635192019a1b01a73ec87ff1b2453b05e89362b542.
The separate source/test root on Welsch is
`/home/ctox/.cache/ctox/file-preservation-fbeca32b7/`.
The test unit is `ctox-file-preservation-test-fbeca32b7.service`,
invocation 54a69f0fbf6540a1927f8a375e61df06. It builds the pinned sidecar and
runs the exact native SQLite preservation regression. Source, Cargo target
and temporary test databases are confined to this separate directory.
`test.sh` and `test.log` retain the recipe and output.

The unit has one CPU of quota, Nice=10, MemoryMax=6 GiB and a one-hour runtime
bound; Cargo uses one build job and a 6 GiB virtual-address-space limit.
These resource limits were read back from systemd. The run subsequently failed
with Cargo exit 101 / rustc SIGABRT: `memory allocation of 27148 bytes failed`.
The SQLite test did not execute; this is not a pass. The 6 GiB virtual-address
limit was reached during compilation. Another isolated release-mode regression
unit was already running when this result was inspected, so no duplicate retry
was started. The production service has not been switched to the candidate by
this task.

The existing install rollback also needs explicit handling before activation:
`rollback_to_previous_release` restores the pre-build database backup on
service/unit refresh failures. An executable recovery must not rewind writes
that occurred during a build. Do not invoke that path as an unchecked fallback.

Native initial replication measurements after the restart included:
desktop_file_index 39,324 ms, business_records 47,397 ms and knowledge_tables
50,677 ms. SQLite recorded 12,519 statements, 155.417 s cumulative time and
1.028 s maximum; maximum writer-lock wait was 0.955 ms. These are individual
native observations, not browser boot p95 or command p50. Current performance
acceptance is failing/open.
