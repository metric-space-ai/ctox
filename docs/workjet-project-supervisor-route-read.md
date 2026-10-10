# Configured project Supervisor route display

Native Owner-only commands:
- `ctox.workjet.project.supervisor.route.capabilities.v1`
- `ctox.workjet.project.supervisor.route.read.v1`

Both accept only `project_id`, `thread_id` and the existing optional native `inbound_channel`. Current native project ownership, exact Supervisor binding and DataRead policy apply. The old strict v1 turn capabilities are unchanged.

Shell entry: `globalThis.workjetProjectControl`.
Use `project.supervisor.route.capabilities.v1` first, then `project.supervisor.route.read.v1`. Both requests contain `action, commandId, projectId, threadId`. The authenticated guest supplies actor/instance authority; callers cannot supply a model, account or actor. The native command receipt must match the exact operation, payload, project and thread after revalidating the guest throughout collection readiness and the response wait.

Shell responses contain `action, commandId, projectId, threadId, contract` plus:
- `capabilities`: schema `ctox.workjet.supervisor.route-capabilities.v1`, `project_id`, `supervisor_thread_id`, `read_schema`, `read_command`.
- `route`: schema `ctox.workjet.supervisor.route-display.v1`, `project_id`, `supervisor_thread_id`, `configured`, `source`, `actual`.

Both DTOs come from fixture `src/core/rxdb/tests/fixtures/workjet-supervisor-route-display-v1.json`; regenerate native/browser with `node src/core/rxdb/tools/build_workjet_jour_fixe_contract.mjs`. These read controls use the existing authenticated RxDB business command path; there is no HTTP data path or local-thread model fallback.

`configured` contains the current Luma/configuration/computer/harness/route/model/catalog observation. It is not an actual producer witness. `source` is the matching immutable request fingerprint or null. Public errors retain a typed code but never an account/private diagnostic. Account IDs, holder-local selectors and credentials stay private. Deferred reads never create the attempts schema or reserve a Core writer.

`actual` is explicitly null in the current native reader and is required to remain null at the Shell boundary. The reserved future producer type cannot itself prove execution; the private attempts column is deliberately never queried. A positive producer requires its actual admitted native controller, turn and holder receipts.

No selected Luma preserves Monday's instance-default path; this read never starts a model or worker. An unavailable selected route is reported honestly, and must not be displayed as a successful instance-default execution.

The isolated Chromium regression executes the real app control function and helper with simulated session state and native receipts. It checks scope/receipt validation and authority changes; it does not prove an installed authenticated guest, a physical WebRTC connection or a model turn. Installed Workjet acceptance must establish those separately.
