# Business OS App Creation Plan

Purpose: make CTOX Business OS app creation reliable through the real CTOX
Business OS paths. A user should be able to ask CTOX for a small business app
and receive a runtime-installed, immediately runnable vanilla HTML/CSS/browser
ESM app that persists data through CTOX DB and can dispatch normal CTOX
Business OS automation commands.

This is a working plan. Update it during execution, not only at handoff.

## Update Protocol

Every agent that works on app creation must update this file before handing off.
If the work is interrupted, update it before switching tasks.

Update these sections when work changes:

- Current State
- Active Slice
- Phase Tracker
- Bench Matrix
- Evidence Log
- Open Issues
- Next Actions

Rules for edits:

- Keep the newest factual status at the top.
- Record commands, run ids, release ids, commits, files, and evidence paths.
- Mark a phase `done` only after its exit criteria have evidence.
- Keep one active slice. When starting work, set `Active Slice`; when finishing,
  update the checklist and add an Evidence Log entry.
- Mark a failure class before patching anything.
- Do not use this plan as an app-building prompt.
- Do not expand this file into a long prompt or rule wall. Keep it operational.

Status values: `pending`, `in_progress`, `blocked`, `done`.

## Current State

Last updated: `2026-06-21`

Overall status: `in_progress`, not production-ready.

Current CTOX installed release:

- Active install: `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T014938Z`
- Source head: `aa945a71 Accept namespaced Business OS app actions`
- Upgrade path: `ctox upgrade --dev` completed and applied
  `branch-main-20260621T014938Z`.
- Business OS shell assets are now byte-identical across source, state, and runtime:
  `src/apps/business-os/app.js`,
  `/Users/michaelwelsch/.local/state/ctox/business-os/app.js`, and
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app.js`
  all have SHA-256 `ec25ef4fd0ded5994c4aae5f529ad199e73b1da3857098da648807eae4d28ed7`.

Current proof run:

- Run id: `rfix6`
- Suite: `core-five`
- Model: `minimax-m3`
- Context: `256k`
- Entry path: real `ctox.business_os.app.create` tasks through installed CTOX.
- Evidence dir: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6`
- Latest static status snapshot: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/status-1782008390263.json`
- Browser evidence: pending for `rfix6`.

Latest live result:

- `rfix6` is running: Subscriptions, Inventory, and Projects are `handled` and
  installed-validation green; Contracts and Quality are still `pending`.
- CTOX is currently `running=true`, `busy=false`, `worker_active_count=0`, and
  `pending_count=2`. This is a reproduced `runtime_orchestration_gap`: after a
  handled durable app-create task, the service does not automatically lease the
  next pending app-create task. Clean service restart has kicked the next task,
  but restart is a workaround, not production readiness.
- Installed validator now rejects unscoped runtime collection names and accepts
  valid namespaced `data-*-action="new"` create affordances. Historical `rfix5`
  apps such as `bench_inventory_rfix5` are correctly red because they use
  shared/domain collection names.
- Installed module catalog contains all five `rfix5` modules as `source=installed`.
- Latest recorded `ctox status --json` showed Business OS web/MCP autostarted,
  native RxDB peer `replicationUp=true`, and `http_bridge_available=false`.
  Recheck `busy` before any `ctox upgrade --dev`, service restart, or fresh
  bench run.
- Browser mount is proven for all five apps, but first mount from a fresh
  browser context can take roughly 25-47 seconds while the browser waits for the
  RxDB module catalog.
- Browser E2E is not production-green yet. Historical `rfix5` browser evidence:
  - Subscriptions: UI save, reload persistence, native DB sync, and
    `business_os.chat.task` dispatch green.
  - Inventory: UI save reports success and the row is visible after reload, but
    the row does not reach native SQLite. Root cause: generated runtime
    collections are not module-id scoped and collide with previous bench schemas
    such as `bench_inventory_items`.
  - Projects: targeted create/save/native-sync/follow-up passed when the
    browser smoke clicked the detail button and waited for command replication.
    The earlier red result was a browser-smoke selector/wait gap.
  - Contracts: UI save, reload persistence, native DB sync, and
    `business_os.chat.task` dispatch green.
  - Quality: targeted UI save, reload persistence, and native DB sync passed
    with a longer sync wait. The earlier red result was a browser-smoke wait gap.
- Fresh `rfix6` browser E2E is pending until all five tasks are terminal.

## Non-Negotiables

The app creation path must stay simple and agent-led.

Do:

- Let CTOX create apps through normal queue/command execution and the Business
  OS app-module skill resources.
- Use runtime-installed app files under the CTOX install runtime, for example
  `$CTOX_INSTALL_ROOT/current/runtime/business-os/installed-modules/<module-id>`.
- Build apps as vanilla `index.html`, `index.css`, and browser ESM `index.js`
  plus small ESM helper files when useful.
- Persist app data only through shell-provided `ctx.db` collection handles.
- Dispatch automation only through `ctx.commandBus.dispatch`.
- Keep `module.json`, `collections.schema.json`, and `schema.js` in exact
  collection/version/type parity.
- Use existing Business OS apps as references; the skill must require at least
  three concrete reference apps before implementation.
- Patch source, validation, skill resources, or CTOX runtime only when evidence
  shows a systemic gap.
- Install source fixes through `ctox upgrade --dev`.

Do not:

- Do not reintroduce a deterministic app builder or file writer.
- Do not repair generated app artifacts by hand.
- Do not write runtime app output into `src/`; `src/` holds source and store
  templates only.
- Do not import upstream `rxdb`, React, Next.js, IndexedDB wrappers, package
  manager dependencies, or build-time frameworks.
- Do not add HTTP data bridges or fallbacks for Business OS app data.
- Do not add long prompt blocks inside the skill.
- Do not copy source-only manifest fields from built-in modules into runtime
  app manifests.
- Do not add UI slop: unused third columns, fake actions, nonfunctional
  buttons, hidden overlays that still intercept clicks, or resize CSS without a
  real layout need.

## Production Gates

App creation is production-ready only when every gate below is green.

| Gate | Status | Required Evidence |
| --- | --- | --- |
| Skill shape | in_progress | English, concise, resource-based, no prompt wall, requires three reference apps, clear Do/Don't list, clear green checklist. |
| Correct install location | done | `rfix5` apps are under `runtime/business-os/installed-modules/<module-id>`; shell asset refresh now preserves installed modules during `ctox upgrade --dev`. |
| CTOX-native creation | done | Five bench apps were created through real Business OS app-create tasks, not direct file writes. |
| Static validation | in_progress | Installed validator now rejects historical unscoped `rfix5` apps; `rfix6` has 3/5 apps handled and validation green, with 2 pending. |
| Browser mount | done | All five apps mounted from installed runtime paths in fresh browser contexts with no console/page/request failures; mount latency remains an open quality issue. |
| Persistence | in_progress | Historical targeted proof reached native SQLite for Subscriptions, Projects, Contracts, and Quality; `rfix6` browser persistence proof is still pending. |
| Automation | in_progress | Subscriptions, Contracts, and targeted Projects created `business_os.chat.task`; fresh `rfix6` browser automation proof is still pending. |
| Entry-point coverage | pending | Chat, App Creator, App Store/template flow, CLI, and inbound/MCP paths all attach the same app-module skill/resource context. |
| Install/upgrade lifecycle | in_progress | `ctox upgrade --dev` applied `branch-main-20260621T014938Z`; source/state/runtime shell assets match; service is running, but durable app-create queue drain is not production-green. |
| No regressions | in_progress | Validator regressions are patched, but queue drain and fresh browser E2E must block production signoff. |

## Phase Tracker

| Phase | Status | Owner | Exit Criteria | Evidence |
| --- | --- | --- | --- | --- |
| 0. Remove deterministic builder | done | Codex | App Creator creates durable app-create tasks instead of writing app files directly. | Earlier commits: `e8bec3b8`, `b142e4c8`; installed path verified in later bench runs. |
| 1. Simplify skill/resources | in_progress | Codex | Skill is English, concise, reference/resource based, and avoids prompt walls. | Current skill/resources were simplified, but final entry-point proof is still open. |
| 2. Build CTOX-native bench | done | Codex | Bench submits real app-create tasks and records evidence without creating or repairing app files. | `ctox business-os app bench run/status`; run dirs under `runtime/business-os/app-creation-bench/`. |
| 3. Close lifecycle/orchestration gaps | in_progress | Codex | Queue, validation, launchd/dev-upgrade, module catalog, and native peer lifecycle work without manual service recovery. | `f009a3b4` and `bbbdbbd4` fixed earlier lifecycle issues; `rfix6` now reproduces a queue-drain gap after handled app-create tasks. |
| 4. Close validator/resource gaps | done | Codex | Validator rejects known bad app artifacts before browser E2E finds them; skill resources state the same architecture expectations plainly. | `0dd04c31` rejects unscoped collections; `aa945a71` accepts and checks namespaced `data-*-action`; installed release `branch-main-20260621T014938Z`. |
| 5. Fresh five-app CTOX proof | in_progress | Codex | One fresh post-validator run with five apps reaches terminal queue success and installed validation green. | `rfix6`: Subscriptions, Inventory, and Projects handled/validation green; Contracts and Quality pending because the queue does not drain automatically. |
| 6. Browser proof | pending | Codex | Browser mount, UI persistence, reload persistence, and automation smoke pass for all five fresh apps. | Pending fresh post-validator bench. |
| 7. Entry-point proof | pending |  | Every user-facing app creation/modification path uses the same skill/resource context and runtime app contract. | Not done. |
| 8. Production signoff | pending |  | All production gates are green, latest source is installed, plan and docs updated, no unrelated dirty files staged. | Not done. |

Phase editing rules:

- A phase may move to `done` only with an Evidence Log entry and at least one
  command, file, run id, or browser evidence path.
- A failed browser or validation run must update the Bench Matrix and Open
  Issues before any fix is applied.
- A source fix must update Next Actions with the exact verification that will be
  rerun after `ctox upgrade --dev`.
- A generated app failure must not be fixed inside the generated app directory
  unless this plan explicitly records that the app is disposable forensic
  evidence and not part of the proof run.

## Active Slice

Owner: `Codex`

Active phase: `5. Fresh five-app CTOX proof`

Current rule: do not patch generated app artifacts by hand. `rfix6` is the
active post-validator bench. Continue it through normal CTOX queue handling, but
treat the idle service with pending app-create tasks as a source/runtime issue.
Clean restart may be used only to gather evidence for Contracts and Quality; it
must not be counted as the production fix.

Immediate checklist:

- [x] Confirm installed releases after source fixes, including
  `branch-main-20260621T001015Z`, `branch-main-20260621T012600Z`, and
  `branch-main-20260621T014938Z`.
- [x] Confirm `rfix5` was submitted through real app-create tasks.
- [x] Confirm all five apps are handled and installed-validation green.
- [x] Confirm source/state/runtime Business OS shell assets match after
  `ctox upgrade --dev`.
- [x] Run browser mount smoke for all five apps.
- [x] Run browser UI save, reload persistence, native DB sync, and automation
  smoke where each app can reach the next step.
- [x] Classify Inventory UI-save failure.
- [x] Classify Projects follow-up dispatch failure.
- [x] Classify Quality browser-to-native sync failure.
- [x] Decide whether each fix belongs in skill resources, static validator,
  runtime/shell, or app-review rework behavior.
- [x] Patch the smallest systemic gap.
- [x] Commit and push the source validator/skill patch to `main`.
- [x] Run `ctox upgrade --dev` after the patch is on `main`.
- [x] Verify the installed validator rejects historical unscoped `rfix5`
  modules.
- [x] Start a fresh CTOX five-app bench after the systemic fixes.
- [x] Install the namespaced `data-*-action` validator patch through
  `ctox upgrade --dev`.
- [x] Confirm `rfix6` Subscriptions, Inventory, and Projects are handled and
  installed-validation green.
- [x] Classify the repeated idle-with-pending queue state as
  `runtime_orchestration_gap`.
- [ ] Patch the queue-drain gap in CTOX source, not in generated apps.
- [ ] Verify the queue-drain fix with a targeted Rust regression test and a
  fresh or continued real CTOX bench.
- [ ] Drain or classify the two pending `rfix6` queue tasks: Contracts and
  Quality.
- [ ] Repeat browser E2E until all five apps are green.
- [ ] Update this file after every material result.

Current slice exit criteria:

- Queue-drain behavior is fixed or explicitly blocked with source-level
  evidence.
- Contracts and Quality leave `pending` without relying on manual restart as the
  production mechanism.
- All five `rfix6` apps are either handled and validation green, or each failure
  has one failure class and one next patch target.
- Browser E2E is run only after the static/queue proof is terminal for all five
  apps.

## Bench Matrix

Active run `rfix6`:

| Case | Module Id | Queue Task | Queue Status | Static Validation | Browser E2E | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix6` | `queue:system::b1d790469d5c9dae5978257f` | handled | green | pending | Rework fixed module tests and app now validates green. Installed validator also covers namespaced `data-*-action`, though this generated app currently uses generic `data-action="new"`. |
| Inventory | `bench_inventory_rfix6` | `queue:system::0443816ee994cd5b2a753272` | handled | green | pending | Generated collections are module-scoped (`bench_inventory_rfix6_*`); old `rfix5` collection drift did not recur. |
| Projects | `bench_projects_rfix6` | `queue:system::c46914ae3d7379543d66c59c` | handled | green | pending | Completed after app-validation rework; MiniMax/tool-call invalid-params events occurred during rework and remain forensic context. |
| Contracts | `bench_contracts_rfix6` | `queue:system::d3e25efcdcf4af03dac40faa` | pending | skipped | pending | CTOX service is idle with task pending; this is part of the queue-drain gap. |
| Quality | `bench_quality_rfix6` | `queue:system::f67604fe551242397dd366d5` | pending | skipped | pending | CTOX service is idle with task pending; this is part of the queue-drain gap. |

Historical run `rfix5`:

| Case | Module Id | Queue Task | Queue Status | Static Validation | Browser E2E | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix5` | `queue:system::1f1fc12323db6d76c2d82b4f` | handled | green, 29/29 tests | green | UI save, reload, native DB row, and `business_os.chat.task` command green. |
| Inventory | `bench_inventory_rfix5` | `queue:system::ba7b80e7822eba234b516731` | handled | old installed validator green; source-patched validator red | red | `inventory-extended-diagnostic-1782003820176.json`: UI save succeeds, row appears after reload, native row remains `0`; unscoped collections are the root architecture gap. |
| Projects | `bench_projects_rfix5` | `queue:system::0a6a72bdb295789065e82cf0` | handled | old installed validator green; source-patched validator red | targeted green | `projects-button-diagnostic-1782003721310.json`: new project native row `1`, command row becomes `1` after detail-button click and replication wait. |
| Contracts | `bench_contracts_rfix5` | `queue:system::265eb0e3dce584b5352ae416` | handled | green, 48/48 tests | green | UI save, reload, native DB row, and `business_os.chat.task` command green. |
| Quality | `bench_quality_rfix5` | `queue:system::19327b78a8dda35d19ce3cea` | handled | old installed validator green; source-patched validator expected red | targeted green | `quality-diagnostic-1782003488353.json`: UI save, reload visibility, and native row `1` green with longer sync wait. |

Historical runs:

- `rcli`: exposed private app lifecycle/visibility gap and reference mismatch.
- `rfix1`: exposed source-reference catalog gap.
- `rfix2`: proved dynamic runtime collection registration, then exposed browser E2E validator gaps.
- `rfix3`: exposed duplicate runtime functions and missing Save/Submit controls.
- `rfix4`: exposed schema/record parity drift; now correctly red under the installed validator.
- `rfix5`: static creation proof is green, browser E2E is red and must not be
  used for production signoff.

Only the latest fresh post-fix run may be used for production signoff.

## Failure Classification

Use these classes before patching:

- `model_failure`: one generated app has inconsistent logic/tests or poor UI,
  but the validator/skill/runtime did not miss a reusable CTOX architecture rule.
- `skill_resource_gap`: multiple apps miss the same CTOX-specific architecture
  concept, or the skill/resource wording makes the correct path unclear.
- `validator_gap`: bad artifacts pass installed validation but fail predictable
  browser/static/runtime checks.
- `runtime_orchestration_gap`: CTOX queue, app lifecycle, install path,
  launchd/systemd, module catalog, native peer, or validation finalization is wrong.
- `data_plane_gap`: WebRTC/CTOX DB/schema registration/sync is wrong.
- `entry_point_gap`: a user-facing path does not attach the same skill/resource
  context or does not create a normal durable app-create task.

Patch policy:

- Patch the skill only for repeated or clearly reusable app-building guidance.
- Patch the validator when it can reject a concrete bad artifact generically.
- Patch CTOX runtime when the app output is valid but lifecycle/data/queue
  machinery fails.
- Do not patch generated app files.

## Architecture Translation Cheatsheet

| Common web-app assumption | Business OS app equivalent |
| --- | --- |
| Next.js/React app with build step | Vanilla runtime module: `index.html`, `index.css`, browser ESM `index.js`. |
| npm/package dependency | No dependency management. Only browser ESM files that are shipped with the app or provided by the shell. |
| App-owned database setup | Shell supplies `ctx.db`; app registers declared collections and uses supplied handles. |
| IndexedDB/Postgres direct access | CTOX DB over WebRTC; never an HTTP bridge and never an app-owned IndexedDB wrapper. |
| REST API write | `ctx.db.<collection>` write or `ctx.commandBus.dispatch` for Business OS commands. |
| Queue/task/ticket side effect | Dispatch a normal Business OS command, commonly `business_os.chat.task` or an allowed ticket command. |
| Framework router/layout | Business OS shell mounts one module. Keep layout simple; use modals where appropriate. |
| Source app template | Reference only. Runtime apps must adapt to runtime manifest/schema rules. |

## Finalization Checklist For Each New App

Use this before marking any generated app green:

- [ ] Files exist: `module.json`, `collections.schema.json`, `schema.js`, `index.html`, `index.css`, `index.js`, `icon.svg`, `locales/en.json`, `locales/de.json`, tests, and helper ESM where needed.
- [ ] App is in the runtime installed-module directory, not `src/`.
- [ ] `module.json`, `collections.schema.json`, and `schema.js` agree.
- [ ] Runtime collection names are scoped to the module id.
- [ ] Record helper outputs match declared JSON types.
- [ ] No package manager, build step, React/Next/Vue, upstream `rxdb`, or HTTP data bridge.
- [ ] UI has a primary create/edit path for an empty state.
- [ ] Every visible button either works or is removed.
- [ ] Hidden modals/overlays are actually hidden and cannot intercept clicks.
- [ ] No ornamental third column unless the app genuinely needs it.
- [ ] No resize-column CSS unless the implemented layout actually supports it.
- [ ] Browser mount has no console/page/request failures.
- [ ] UI create/edit persists through `ctx.db`, reload, and native CTOX DB sync.
- [ ] Automation dispatches through `ctx.commandBus.dispatch` and creates a normal command record.

## Next Actions

1. Inspect and patch the durable queue idle/finalization path in CTOX source so
   a handled app-create task leases the next pending app-create task without a
   manual service restart.
2. Add or update a targeted Rust regression test for this exact condition:
   one app-create task completes validation, `busy=false`,
   `worker_active_count=0`, and another pending app-create task is leased or
   enqueued automatically.
3. Run the narrow verification for the queue fix, then install through
   `ctox upgrade --dev`. Do not use a production/runtime env toggle.
4. Continue `rfix6` only to collect evidence. If a clean restart is used to
   kick Contracts or Quality, record it as workaround evidence and do not mark
   Phase 3 done.
5. After all five `rfix6` tasks are terminal and validation green, run browser
   E2E with precise selectors and longer native/command sync waits.
6. Keep browser-smoke failures separate from generated app failures: record
   selector/wait gaps as harness issues, not app defects.
7. Update this file before every handoff, before every source patch, and after
   every material bench result.

## Evidence Log

- `2026-06-20`: schema/record parity validator landed in commit `ebfba103` and was installed as `branch-main-20260620T220404Z`. Historical `rfix4` is correctly red under that validator.
- `2026-06-20`: fresh run `rfix5` started through installed CTOX with `minimax-m3`, `256k`, and five real app-create tasks.
- `2026-06-20`: commit `3ce6863d` fixed green app-validation rework completion in source and expanded command-bus alias validation. Verification passed: `node src/apps/business-os/scripts/validate-app-module.test.mjs`, `cargo test --bin ctox business_os_app_validation_`, `cargo test --bin ctox worker_finalization_`, targeted rustfmt/diff checks, and `cargo check --bin ctox`.
- `2026-06-21`: commit `f009a3b4` fixed managed-install shell asset refresh while preserving runtime installed modules and bench evidence.
- `2026-06-21`: commit `bbbdbbd4` fixed the `install.sh --rebuild` path used by `ctox upgrade --dev`; upgrade applied `branch-main-20260621T001015Z`.
- `2026-06-21`: source, state, and runtime `business-os/app.js` are all SHA-256 `ec25ef4fd0ded5994c4aae5f529ad199e73b1da3857098da648807eae4d28ed7`.
- `2026-06-21`: static status snapshot `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1782001080447.json` shows `rfix5` at `bench_green=true`, `handled=5`, and `validation_passed=5`.
- `2026-06-21`: browser evidence `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/browser-smoke/subscriptions-canary-1782002040950.json` shows Subscriptions full E2E green.
- `2026-06-21`: browser evidence `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/browser-smoke/browser-smoke-1782001975201.json` shows Contracts full E2E green and Inventory, Projects, Quality red as listed in the Bench Matrix.
- `2026-06-21`: targeted Projects browser evidence `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/browser-smoke/projects-button-diagnostic-1782003721310.json` shows create/save/native row green and `business_os.chat.task` command row appearing after detail-button click plus wait.
- `2026-06-21`: targeted Quality browser evidence `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/browser-smoke/quality-diagnostic-1782003488353.json` shows UI save, reload visibility, and native complaint row green.
- `2026-06-21`: targeted Inventory browser evidence `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/browser-smoke/inventory-extended-diagnostic-1782003820176.json` shows UI save success and reload visibility but native item row remains `0`, matching the unscoped collection-name drift finding.
- `2026-06-21`: local validator patch added module-scoped collection enforcement. Verification passed: `node src/apps/business-os/scripts/validate-app-module.test.mjs`. Source validator now correctly rejects historical `bench_inventory_rfix5` and `bench_projects_rfix5` installed artifacts for unscoped collection names.
- `2026-06-21`: installed release check showed `/Users/michaelwelsch/.local/lib/ctox/current` points to `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T011143Z`. `ctox status --json` reports `running=true`, `busy=false`, native RxDB peer `replicationUp=true`, and `http_bridge_available=false`. The installed validator still needs the local patch via push to `main` plus `ctox upgrade --dev`.
- `2026-06-21`: commit `0dd04c31` pushed to `main` with the app-creation plan, module-scoped collection validator, validator regression test, and matching skill resource updates.
- `2026-06-21`: commit `c3807c87` pushed the plan update after the validator push.
- `2026-06-21`: `ctox upgrade --dev` applied `branch-main-20260621T012600Z` from `main`. Installed validator check confirmed `normalizedModuleCollectionPrefix` and the `must be scoped to module id` failures are present. `ctox business-os app validate bench_inventory_rfix5 --installed --skip-tests --skip-node-check` fails as expected on unscoped `bench_inventory_*` collections. Follow-up `ctox status --json` reports `running=true`, `busy=false`, native RxDB peer `replicationUp=true`, and `http_bridge_available=false`.
- `2026-06-21`: `rfix6` submitted through installed CTOX with five real app-create tasks and removed old `rfix5` modules. Evidence dir: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6`.
- `2026-06-21`: `rfix6` snapshot `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/status-1782006463792.json` shows Subscriptions `handled` with validation green and Inventory/Projects/Contracts/Quality still `pending`.
- `2026-06-21`: Source validator patch added namespaced `data-*-action` create-affordance and dead-button handling. Verification passed: `node src/apps/business-os/scripts/validate-app-module.test.mjs`; source validator also passes `bench_subscriptions_rfix6` installed validation.
- `2026-06-21`: commit `aa945a71` pushed to `main` with namespaced `data-*-action` validation and regression coverage.
- `2026-06-21`: `ctox upgrade --dev` applied `branch-main-20260621T014938Z`. Installed validator contains the namespaced action handling and Business OS remains on native RxDB/WebRTC with `http_bridge_available=false`.
- `2026-06-21`: `rfix6` snapshot `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/status-1782008390263.json` shows Subscriptions, Inventory, and Projects `handled` with installed validation green; Contracts and Quality remain `pending`.
- `2026-06-21`: live `ctox status --json` after the third handled `rfix6` app reports `running=true`, `busy=false`, `worker_active_count=0`, `pending_count=2`, native RxDB peer `replicationUp=true`, and `http_bridge_available=false`. The repeated idle-with-pending state is classified as `runtime_orchestration_gap`.

## Open Issues

- Runtime orchestration is not production-ready: after handled app-create tasks,
  CTOX can become idle while later app-create tasks remain pending. Clean restart
  kicks the next task, proving the pending tasks are leaseable, but restart is
  not an acceptable production mechanism.
- Historical `rfix5` artifacts are invalid under the hardened installed
  validator because runtime collections are not module-id scoped.
- Browser E2E for Inventory must be rerun on `rfix6`, where generated
  collections are module-scoped, before the old native-sync issue can be closed.
- Fresh-browser mount works but takes 25-47 seconds in observed cases; decide whether that is acceptable or a shell catalog readiness issue.
- Historical repeated bench runs produced domain-level collection names such as
  `bench_inventory_items`; this was classified as `skill_resource_gap` plus
  `validator_gap` and is patched/installed.
- Browser-smoke selector/wait precision is weak: Projects needed a scoped detail
  button selector and command replication wait; Quality needed a longer native
  sync wait.
- `rfix6` currently has two pending tasks while CTOX reports `busy=false`.
  Continue only after recording the queue-drain fix path, or use restart solely
  as evidence collection.
- Verify App Creator, Chat, App Store/template, CLI, and inbound/MCP entry paths all use the same app-module skill/resource context.
- Confirm the skill remains short, English, resource-based, and not a prompt wall.
- Keep unrelated dirty file `tests/business-os/ats_synthetic_generate.sh` out of this work unless explicitly requested.
