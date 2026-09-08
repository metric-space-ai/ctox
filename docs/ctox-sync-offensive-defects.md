# CTOX Sync offensive: reported defects and acceptance register

Updated 2026-09-08. This register tracks the user's reported failures separately
from the six-stage architecture completion criteria. An implemented patch,
unit test or component fixture does not close a production incident.
No row below constitutes tenant acceptance. Office application implementation
belongs to task `01a06c90-1100-7903-892d-667764f7eb2f`; shell, sync and shared
command delivery defects remain in this offensive.

## Reported defects

| ID | User-visible defect | Required closing evidence | Verified state / next action |
|---|---|---|---|
| D01 | Inconsistent shell releases overwrite one another; ctox.dev interferes with instance shell delivery. | Trace one instance-owned, verified release through proxy, native serving slot and actual loaded Web/Desktop/Mobile assets; reject incompatible/corrupt versions visibly. No silent archive fallback. | Open. No tenant deployment or complete cross-host release acceptance established by the current PR. |
| D02 | Real *.ctox.dev instances fail startup; welsch.ctox.dev reports missing/outdated CTOX-DB bundle. | Real authenticated startup and reload with matching shell/runtime/schema provenance and successful app use; verify representative affected tenants. | Open production acceptance. No claim that component checks restore existing tenant instances. |
| D03 | Apps display only parts of their data; CTOX app no longer shows the harness flow. | Actual native harness run and its persisted projections visible in the CTOX UI after reload, including status/history; verify affected data views against native records. | Open. Four-host transport acceptance executes no coding harness and cannot close this defect. |
| D04 | thesen browser leaves 15–16/21 collections pending or stalled after reload. | Reproduce supplied upgrade range; all 21 complete after each reload in <60 s on representative data, then tenant acceptance. | Isolated native/browser fixture passes 21-collection reloads; real tenant and version-range causal attribution remain open. |
| D05 | Incoming transfers stall and concurrent masterChangesSince requests time out across collections. | Transfer/query progress under multiplex load, partitions/reconnect, slow peers and missing chunks, without shared starvation or recurring timeouts. | Open production acceptance. Passing one reload fixture is insufficient for sustained stability. |
| D06 | Researched lead stays at import revision; deleted leads reappear/remain visible. | Compare native revision 23 and researched fields with actual browser view; deliver all tombstones after offline updates/reload. | Isolated fixture verifies revision 1→23, live lead count 36→19 and 3911 thread states. Affected tenant's real records remain unaccepted. |
| D07 | Browser writes partly disappear: 13/19 deletions delivered, import requires IndexedDB wipe, hundreds of unacknowledged journal writes. | All writes acknowledged or explicitly rejected; reload/reconnect/restart preserves pending journal and exactly one effect without a cache wipe. | Open. Queue transaction/rollback tests and 30 successful control commands do not prove the reported interrupted batch workflow. |
| D08 | cockpit-projections consumes 69–100% CPU; direct-session drops hundreds of events. | Reproduce representative workload; measure per-thread CPU, projection throughput and event loss before/after, including idle behavior. | Open; no current CPU/event-loss acceptance. |
| D09 | Clicking an action that submits a prompt takes too long to open the chat. | Real UI click → visible pending prompt measured separately from native command acceptance and terminal projection; preserve prompt/context/draft under slow storage. | Initial synchronous chat render implemented in e1f5d4c5a. 42 component browser cases pass; native two-actor workflow in 484b21ef5 FAILS before submission: requester collection authorization errors, masterChangesSince timeouts and a cancelled command collection. Cause is not established. |
| D10 | Warm command delivery is too slow despite local backend roundtrip. | At least 30 real Browser→WebRTC→CTOX terminal commands, complete correlated marks; warm p50 strictly <300 ms, with separate WAN/large-session measurements. | Source e1f5d4c5a run 34257231148: 30 samples, p50 289.5 ms, p95 391 ms PASS. Later runs FAIL: b87249885 p50 340.5/p95 545.65 ms; 484b21ef5 p50 375.5/p95 544.7 ms, each 30/30 complete. Stable performance remains open. |
| D11 | Slow bootstrap and repeated full bootstrap when switching views; mobile suspend is fragile. | Critical collection boot p95 <5 s over 30 runs; warm view switch preserves active session; mobile suspend resumes persisted state. Measure browser/profile/native cohorts separately. | Retained-profile browser cohort passes in run34261603857: 30 reloads, p95 3454.14 ms, all five critical collections complete/live with checkpoint epochs. Fresh-profile, native cold setup, native restart and real mobile acceptance remain open. |
| D12 | Historically damaged data must be removed without losing valid histories, attachments or app records. | Identify exact corrupted records and cause; immutable backup, migration/reconciliation balance, repair/removal on copies, recovery rehearsal, then scoped authorized production repair. | Open; no blanket deletion or cache wipe counts as repair. |
| D14 | Old HTTP-bridge diagnostics can make failed/offline WebRTC collections appear ready. | Reject obsolete receipts as proof of initial sync, streaming or checkpoint epoch; preserve healthy native state behavior. | Acceptance branches removed, regression demonstrated red before/green after removal. Full browser CI pending; no tenant deployment. |
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
- Full production readiness, merge and tenant upgrades are not approved by these
  partial results. Preserve the user's requested final scope; do not close this
  register by relabeling missing acceptance as out of scope.
