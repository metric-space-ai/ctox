# CTOX Sync offensive: reported defects and acceptance register

Updated 2026-09-09. This register tracks the user's reported failures separately
from the six-stage architecture completion criteria. An implemented patch,
unit test or component fixture does not close a production incident.
No row below constitutes tenant acceptance. Office application implementation
belongs to task `01a06c90-1100-7903-892d-667764f7eb2f`; shell, sync and shared
command delivery defects remain in this offensive.

## Latest full-host evidence

PR69 source `7198613f567e3cef2094ac12eebc713c5b9306e4`, run
`34290313600`, full-host job `102275164694`: **FAIL overall**.
Tested merge `27e3f41f978807a0fb09a8e2985b5a2bb2d6d845`; binary SHA256
`ba546cab058042b30ec8534b595da589479df0184e4ed5ae39ec13f84eb552de`.

- Strict native outage passes: one dispatch, no resubmission or collection
  repair, queued receipt in 8169.5 ms. Native command v2/task v3 and browser
  task counts each equal one, with both cross-references matching. The
  preceding canonical-schema run34285060254 independently passed in7418.1ms.
  Neither run executes a coding harness or closes the interrupted batch incident.
- Warm30-command p50 402ms / p95 537.3ms remains above the300ms p50 limit.
  Critical30-reload p95 3284.88ms passes the5000ms gate. Context fails at
  direct-denial with a browser command still pending_sync; no final canonical
  denied-command row was retained. Browser metadata does not prove admission.
- Context native CPU averages169.12% of one core over128.01 measured seconds.
  No thread-limit omissions/read errors; observer cost~0.72%. RxDB thread names
  identify load, not a source callsite. SQLite statement timers include visitor
  and decoding time; projection timers include outer waits. The new bounded
  owned-PID symbol profiler awaits actual Linux results and does not replace
  unprofiled performance gates.
- Complete bounded actor/native diagnostics were retained in this real failing
  run. The native heartbeat was fresh and reported replicationUp while browser
  queries and writes remained delayed. A fixture denial timeout exposed its
  ephemeral capability; the diagnostic now returns selected command fields.
- The legacy migration-version mode still assumes obsolete commandv1/taskv0
  tables. Replacing constants alone would not prove preserved-data migration;
  populated historical fixtures, inventory comparison and recovery remain open.

Migration source audit reproduced an equal-clock legacy upsert overwriting a
target deletion marker in real SQLite. The candidate now rejects divergent
equal-clock rows, rolls back that table and retains the source; exact retries
perform no rewrite. Full native regression execution for this change is pending.
Its new CI gate includes the existing migration tests, correcting their obsolete
queue-task target from v2 to the registered v3. This is not migration acceptance.

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
