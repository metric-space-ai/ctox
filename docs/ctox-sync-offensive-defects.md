# CTOX Sync offensive: reported defects and acceptance register

Updated 2026-09-09. This register tracks the user's reported failures separately
from the six-stage architecture completion criteria. An implemented patch,
unit test or component fixture does not close a production incident.
No row below constitutes tenant acceptance. Office application implementation
belongs to task `01a06c90-1100-7903-892d-667764f7eb2f`; shell, sync and shared
command delivery defects remain in this offensive.

## Latest full-host evidence

Previous completed full-host cohort: source `cbabdb48d064d70aee2a9e84733cee05ba950a33`,
[run34298240845](https://github.com/metric-space-ai/ctox/actions/runs/34298240845),
tested merge `27c0caaf03ae4ffbd3f25393c9123e9fd9c1945d`, binary SHA256
`3aefce45b2d80d86c9aa0699db344a95f956273a7570b7a083282da613b316a0`.
Overall **FAIL**: context workflow remains red. Warm30 p50 268 ms / p95
354.55 ms and critical30 boot p95 2737.793028 ms pass. All ten native migration
regressions pass, including transactional cleanup and the forced-repair cases.
The isolated profiler captured actual samples; its workflow step fails because
the context fixture fails, not because symbol recording is unavailable.
This cohort precedes the bounded-reader change and cannot prove its effect.

Latest completed reader candidate `e5973c8c315172b2e71e6e677e72929f928e6e70`,
[run34300623670](https://github.com/metric-space-ai/ctox/actions/runs/34300623670):
overall **FAIL**. Native Linux/macOS, Chromium, migration, four-host control,
strict outage and 21-collection reload/restart pass. Linux RxDB passes 400 tests,
including both new reader regressions. Warm30 p50 311.5 ms / p95 387.4 ms fails;
critical30 boot p95 2707.798889 ms passes. Strict outage receives the queued
command after 2590.2 ms, one native command/task, no repair or resubmission.
Context fails at `context-data`, with concurrent masterChangesSince timeouts.
Its shorter 167.41-second CPU cohort averages 108.13% of one core and is not a
comparable successful workload. The separate actual CTOX profile captures 1336
samples with zero lost; parser/trigger duplication, allocator and mutex symbols
remain prominent. Its step preserves the failing context fixture's exit code.

Tested merge `e500b2a6f27cb05b43fdc532ce21642b88413257`; binary SHA256
`fbb82a1ec95cf10d2fc952995e2ccebfa0d86682c4933692b259b38bf48db6ea`.
Source inspection identified a second repeated Core open in every secret
master-key legacy check. The next candidate reuses the KV reader connection
under Unix while rereading every value, including late conflicts and deletion.
A local actual-module fixture passes three tests and measures 30 cached reads
at p50/p95 23/44 microseconds versus 1454/2250 with fresh opens. Full native
secret/authority semantics and browser performance of this candidate are pending.
These results do not close tenant incidents or full portability acceptance.

### Earlier retained full-host and source-attribution cohort

PR69 source `67de21dc96bcd7511a14304dbe2acd16f35ddb48`, run
`34296338920`, full-host job `102293679231`: **FAIL overall**.
Tested merge `de3d33afd51525ca273bee4d55b699e21f264546`; binary SHA256
`f1299e4565fafef7cfeab47705284898c93da4b55d9d725058816bd8d15c1150`.

- Strict native outage passes: one dispatch, no resubmission or collection
  repair, queued receipt in 8231 ms. Native command v2/task v3 and browser
  task counts each equal one, with both cross-references matching.
  This does not execute a coding harness or close the interrupted batch incident.
- Warm30-command p50 404 ms / p95 577.45 ms FAILS the 300 ms p50 gate.
  Critical30-reload p95 3538.10 ms PASSES the 5000 ms gate.
  The 21-collection reload/restart check also passes.
- Context fails in phase `context-app`, with concurrent masterChangesSince
  timeouts. Its native process averages 149.98% of one core over 381.13 measured
  seconds, observer cost ~0.65%, no thread-limit omissions/read errors.
  The longer failure cohort is not directly comparable to earlier denial-stage
  failures and does not demonstrate a CPU improvement.
- A separate real CTOX user-space profile now succeeds: 2444 samples, zero lost
  samples, 30 seconds at 49 Hz. The flat report contains SQLite schema lookup/
  parser work and mutex operations; it is not a caller stack. Source inspection
  found one cached reader per collection. The new candidate bounds point-read
  caches at four per storage factory and separates the change-feed reader.
  Native correctness and full-host performance of this change remain pending.
- Four-host control acceptance, Linux/macOS native checks, Chromium and all six
  earlier native migration regressions pass. Four-host authority validation
  p50 10.39 ms measures control traffic, not business commands or harness execution.
- The legacy migration-version mode still assumes obsolete commandv1/taskv0
  tables. Realistic copied stores, inventory comparison and recovery remain open.

Migration source audit reproduced an equal-clock legacy upsert overwriting a
target deletion marker in real SQLite. The candidate now rejects divergent
equal-clock rows, rolls back that table and retains the source; exact retries
perform no rewrite. All six native migration regressions pass on source67de21dc9,
including the equal-clock and exact-retry cases. This is not full migration
acceptance; copied realistic stores, inventory comparison and recovery remain open.

A further source-level defect is confirmed in the old cleanup path: active
schema metadata alone allowed it to drop a populated old table. The forced
command-repair test seeded `cmd_stale` only in v0, then expected v0 deletion
without checking that command in v2. Startup's copy pass did not cover every
collection selected by cleanup; the CLI repair did not invoke that copy pass.

The cleanup candidate now performs discovery, declared migration, row verification
and trigger/table removal in one immediate SQLite transaction per collection.
Forced repair retains a unique legacy command; missing rules and equal-clock
tombstone conflicts reject cleanup, and rows written after an earlier copy are
rechecked. The same copy implementation serves both paths. Cleanup failures now
abort native bring-up instead of logging and publishing an incomplete peer.
All ten native migration regressions pass in run34298240845. Migration performance
on realistic copied stores remains pending. This does not establish a tenant incident's cause, cross-store
atomicity, immutable backup, complete migration balance or recovery acceptance.

The older `migration-version-browser-to-rust` mode did not seed a legacy store;
it only checked new command routing against hard-coded command-v1/task-v0 tables.
It now derives active versions from the canonical contract, rejects every older
command table and requires exactly one native command/task with matching links.
The matrix follows the same schema contract and keeps zero-stale-row checks.
The full-host workflow now runs this browser/native routing check separately.
Its execution remains pending; even a pass proves current-schema routing and
absence of old executable tables, not a copied-data migration or recovery rehearsal.

The following historical rows remain incident records; the measurements above
supersede their older fixture observations without closing tenant acceptance.

## Reported defects

| ID | User-visible defect | Required closing evidence | Verified state / next action |
|---|---|---|---|
| D01 | Inconsistent shell releases overwrite one another; ctox.dev interferes with instance shell delivery. | Trace one instance-owned, verified release through proxy, native serving slot and actual loaded Web/Desktop/Mobile assets; reject incompatible/corrupt versions visibly. No silent archive fallback. | Open. No tenant deployment or complete cross-host release acceptance established by the current PR. |
| D02 | Real *.ctox.dev instances fail startup; welsch.ctox.dev reports missing/outdated CTOX-DB bundle. | Real authenticated startup and reload with matching shell/runtime/schema provenance and successful app use; verify representative affected tenants. | Open production acceptance. No claim that component checks restore existing tenant instances. |
| D03 | Apps display only parts of their data; CTOX app no longer shows the harness flow. | Actual native harness run and its persisted projections visible in the CTOX UI after reload, including status/history; verify affected data views against native records. | Open. Four-host transport acceptance executes no coding harness and cannot close this defect. |
| D04 | thesen browser leaves 15–16/21 collections pending or stalled after reload. | Reproduce supplied upgrade range; all 21 complete after each reload in <60 s on representative data, then tenant acceptance. | Isolated native/browser fixture passes 21-collection reloads; real tenant and version-range causal attribution remain open. |
| D05 | Incoming transfers stall and concurrent masterChangesSince requests time out across collections. | Transfer/query progress under multiplex load, partitions/reconnect, slow peers and missing chunks, without shared starvation or recurring timeouts. | Open. Source 51ecc7bf9 fixes a reproduced browser admission defect: six unready handshakes occupied all six active query slots without sending an RPC. Waiting handshakes now stay in the bounded queue; current readiness, owner cancellation and slot bounds are covered by the built-bundle regression. Actual full-host stability acceptance remains pending; in-flight transfer/RPC behavior is unchanged. |
| D06 | Researched lead stays at import revision; deleted leads reappear/remain visible. | Compare native revision 23 and researched fields with actual browser view; deliver all tombstones after offline updates/reload. | Isolated fixture verifies revision 1→23, live lead count 36→19 and 3911 thread states. Affected tenant's real records remain unaccepted. |
| D07 | Browser writes partly disappear: 13/19 deletions delivered, import requires IndexedDB wipe, hundreds of unacknowledged journal writes. | All writes acknowledged or explicitly rejected; reload/reconnect/restart preserves pending journal and exactly one effect without a cache wipe. | Open. Run 34276484717 delivered one command after native outage in 8280 ms without resubmission or collection repair, but the final SQLite assertion failed on an obsolete v0 command table. The subsequent ddffd50e2 assertion still used obsolete v1/v0 tables: the canonical contract is command v2 / queue v3. Restart checks now derive both table names from that contract; complete durable handoff acceptance is still pending. Neither this single-command scenario nor transaction tests close the interrupted batch/journal incident. |
| D08 | cockpit-projections consumes 69–100% CPU; direct-session drops hundreds of events. | Reproduce representative workload; measure per-thread CPU, projection throughput and event loss before/after, including idle behavior. | Open. Run34285060254 measured sustained RxDB-peer CPU (149.13% of one core), while its cockpit thread used only 0.290 CPU seconds. The tenant cockpit symptom was not reproduced. A separate context reproduction now requests bounded flat user-space symbol sampling after all acceptance measurements; real Linux symbol results, source attribution and representative tenant CPU/event-loss acceptance remain pending. |
| D09 | Clicking an action that submits a prompt takes too long to open the chat. | Real UI click → visible pending prompt measured separately from native command acceptance and terminal projection; preserve prompt/context/draft under slow storage. | Initial synchronous chat render implemented in e1f5d4c5a. Run 34276484717 on 76b1c7053 measures ask first paint at 119.3 ms (one real-browser sample). The two-actor workflow reaches context-app but exceeds its 300000 ms deadline; screenshot shows the chat reporting interrupted connection. First paint does not prove native acceptance or the complete context workflow. |
| D10 | Warm command delivery is too slow despite local backend roundtrip. | At least 30 real Browser→WebRTC→CTOX terminal commands, complete correlated marks; warm p50 strictly <300 ms, with separate WAN/large-session measurements. | Still failing. Run 34276484717 on 76b1c7053: 30/30 complete marks, p50 392.5 ms / p95 555.55 ms. Earlier 4a842025f: 378/570.7 ms; da52666b5: p50 405.5 ms. The earlier ee477f343 pass (282.5/408.05 ms) did not establish stable performance. Query-fix source 51ecc7bf9 (run 34279536313) also FAILS: 30/30 complete marks, p50 402.5 ms / p95 569.95 ms. Its critical reload p95 is 3515.43 ms, but context-data again exceeds 300000 ms. |
| D11 | Slow bootstrap and repeated full bootstrap when switching views; mobile suspend is fragile. | Critical collection boot p95 <5 s over 30 runs; warm view switch preserves active session; mobile suspend resumes persisted state. Measure browser/profile/native cohorts separately. | Latest completed retained-profile cohort, run34279536313: critical reload p95 3515.43 ms, no report issues. Prior run34276484717: p95 3244.00 ms. Earlier run34261603857: 30 reloads, p95 3454.14 ms, all five critical collections complete/live with checkpoint epochs. These measurements do not certify fresh-profile, native cold setup or real mobile suspend/resume. |
| D12 | Historically damaged data must be removed without losing valid histories, attachments or app records. | Identify exact corrupted records and cause; immutable backup, migration/reconciliation balance, repair/removal on copies, recovery rehearsal, then scoped authorized production repair. | Open; no blanket deletion or cache wipe counts as repair. |
| D14 | Old HTTP-bridge diagnostics can make failed/offline WebRTC collections appear ready. | Reject obsolete receipts as proof of initial sync, streaming or checkpoint epoch; preserve healthy native state behavior. | Acceptance branches removed, regression demonstrated red before/green after removal. Full browser CI pending; no tenant deployment. |
| D15 | Installed Workjet reports `masterWrite conflicts remained for desktop_layout` on two separate runs. | Reproduce on the exact installed release; verify eventual convergence and retained layouts across reload/reconnect without wiping storage. | Open field report from the UI task: app cc6e7e71, shell 0.1.46-beta.8 / 87daa431b1604bfcca364a1eb1a851e90da1874d, latest occurrence 2026-09-09T00:29:55.704Z. App remained usable; source of the conflict and convergence are unverified. Do not attribute this to uninstalled PR changes. |
| D13 | Greppy defects/diagnostic problems prevent reliable use and were not consistently reported. | Reproduction and classification sent to the designated existing task; verify repair against the reproduction. | Report only to 01a07f6a-83e8-7901-851c-36521e4916b4, superseding the stale AGENTS target. Reporting does not imply the Greppy defect is fixed. |

## Architecture requirements that are not closed by the incident fixes

| ID | Required outcome | Remaining acceptance |
|---|---|---|
| A01 | Robust worker enrollment through SSH installation and manual Workjet QR/link pairing over signaling/WebRTC. | Real Desktop and Mobile creation flows, interrupted provisioning/retry, authenticated membership, disconnected/public-network pairing. Workjet PR34 has no completed UI acceptance. |
| A02 | Shared Sync identity/membership and majority-confirmed ownership with OpenRaft 0.9.25, three voters and generation fencing. | Verify host/instance authorization and stale executor rejection at command, gateway and supported tool boundaries; actual process-group stop under authority loss. Four host processes alone do not prove all effect boundaries. |
| A03 | Full Codex/Claude session portability and automatic agent failover. | Complete journal/files/attachments/credentials eligibility, two durable data-peer checkpoint receipts, genuine export/import/resume certification, crash/partition recovery and ambiguous external-effect reconciliation. No silent restart fallback. |
| A04 | One authoritative owner per fact; remove legacy paths after replacement. | Per-path caller/replacement/removal inventory, deletion of obsolete mailbox/ownership/fallback/status-repair paths, executable boundary tests. No permanent parallel writer architecture. |
| A05 | Coordinated data and runtime migration with recovery. | Realistic copied stores, ID/reference/history reconciliation, protected active-session checkpoints, compatible native/client cutover and rehearsed recovery after new writes. |
| A06 | End-to-end release acceptance across Web/Desktop/Mobile. | Actual authentication/logout, tenant/role scoping, visible native data, persistence, console/network health, damaged slot/release outage/mobile suspend, performance and recovery evidence. Unit/component green is insufficient. |

## Evidence and delivery boundaries

- Draft CTOX PR: https://github.com/metric-space-ai/ctox/pull/69
- Draft Workjet PR: https://github.com/metric-space-ai/workjet/pull/34
- Source e1f5d4c5a: https://github.com/metric-space-ai/ctox/actions/runs/34257231148
  — warm command gate passes; old context fixture fails on mismatched peer/command
  actor. That fixture failure is not an application permission-policy regression.
- Source 484b21ef5: https://github.com/metric-space-ai/ctox/actions/runs/34259869288
  — both native platforms and Chromium component job pass. Full-host context
  scenario and warm command budget FAIL; all 21 reload collections complete in
  34.231 / 41.559 / 16.980 s. Full-host failure evidence is retained.
- Source 76b1c7053: https://github.com/metric-space-ai/ctox/actions/runs/34276484717
  — completed full-host FAILURE; tested merge e121076758eb339ee0546672771b0b1c4f28e713,
  binary SHA256 0dfee0d8d767b3f8c909f2e32f935c9f62bcf2654d562595eea7a6176aa2de52.
  Context, warm-command and final native-outage SQLite gates fail as detailed above.
- Query fix 51ecc7bf9: https://github.com/metric-space-ai/ctox/actions/runs/34279536313
  — Linux native, macOS native and Chromium pass; full host FAILS. Tested merge
  a5953a45ebd7f6a508c1defec5af975fdf37adfe; binary SHA256
  e94c18a48a63e3991609b2d7060a1445590626e3c2908275d434c9f517fd9336.
  Context-data times out; warm p50 402.5 ms fails. Strict outage receives a queued
  receipt in 8128.5 ms, then the known obsolete-table assertion fails.
- Corrected outage fixture ddffd50e2:
  https://github.com/metric-space-ai/ctox/actions/runs/34280644714 — FAIL: context-data
  exceeds 300000 ms, warm p50 444 ms; strict outage queued receipt 7909 ms followed
  by absent v1-table failure. Critical reload p95 3270.72 ms passes. Tested merge
  f0fd1373c542cd966608baa258ceffbee4e2a76f, binary SHA256
  4acb3f58bd030671433247f4ef8e86dd19d5b3520715109e5a8f8760527fa3f2.
- Full production readiness, merge and tenant upgrades are not approved by these
  partial results. Preserve the user's requested final scope; do not close this
  register by relabeling missing acceptance as out of scope.
