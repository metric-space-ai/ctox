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
- Source head: `e2e5cf31 Update Business OS app creation plan`
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
- Latest static status snapshot: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/status-1782010063510.json`
- Browser evidence: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/catalog-sync-diagnostic-1782010683224.json`

Latest live result:

- `rfix6` static/install proof is green: all five apps are `handled`, installed
  validation is green for all five, and CTOX is `running=true`, `busy=false`,
  `worker_active_count=0`, `pending_count=0`.
- Native RxDB module catalog contains all five `rfix6` modules and founder
  governance entries for `local-dev`.
- Fresh browser proof is blocked before app-specific E2E: a fresh shell launched
  with `#bench_subscriptions_rfix6` stays on `desktop`, does not show any bench
  module text, and does not mount the generated app even though bootstrap reports
  `http_bridge_available=false` and `native_rxdb_peer_available=true`.
  Preliminary class: `data_plane_gap` or module-catalog visibility gap. Run a
  focused shell-state/RxDB probe before patching source.
- The suspected queue-drain gap is not confirmed by the completed `rfix6` run:
  Contracts and Quality both leased without manual restart after delayed worker
  completion. Keep a latency watch, but do not patch source for queue-drain from
  this evidence alone.
- Installed validator now rejects unscoped runtime collection names and accepts
  valid namespaced `data-*-action="new"` create affordances. Historical `rfix5`
  apps such as `bench_inventory_rfix5` are correctly red because they use
  shared/domain collection names.
- Installed module catalog contains all five `rfix5` modules as `source=installed`.
- Latest recorded `ctox status --json` showed Business OS web/MCP autostarted,
  native RxDB peer `replicationUp=true`, and `http_bridge_available=false`.
  Recheck `busy` before any `ctox upgrade --dev`, service restart, or fresh
  bench run.
- Historical browser mount was proven for earlier generated apps, but fresh
  `rfix6` mount is currently blocked by module-catalog visibility before
  app-specific UI testing can start.
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
- Fresh `rfix6` browser E2E is blocked until the catalog/mount issue is
  classified and fixed or otherwise explained with evidence.

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
| Static validation | done | `rfix6` snapshot `status-1782010063510.json`: handled=5, validation_passed=5, pending=0, failed=0. |
| Browser mount | blocked | Historical app mount was possible, but fresh `rfix6` browser proof is blocked: native catalog has the modules, while the browser shell remains on `desktop` for `#bench_subscriptions_rfix6`. |
| Persistence | in_progress | Historical targeted proof reached native SQLite for Subscriptions, Projects, Contracts, and Quality; `rfix6` browser persistence proof is still pending. |
| Automation | in_progress | Subscriptions, Contracts, and targeted Projects created `business_os.chat.task`; fresh `rfix6` browser automation proof is still pending. |
| Entry-point coverage | pending | Chat, App Creator, App Store/template flow, CLI, and inbound/MCP paths all attach the same app-module skill/resource context. |
| Install/upgrade lifecycle | in_progress | `ctox upgrade --dev` applied `branch-main-20260621T014938Z`; source/state/runtime shell assets match; service is running; queue drain completed `rfix6` without manual restart, but latency should stay under watch. |
| No regressions | in_progress | Validator regressions are patched, but fresh browser module-catalog visibility and full browser E2E block production signoff. |

## Phase Tracker

| Phase | Status | Owner | Exit Criteria | Evidence |
| --- | --- | --- | --- | --- |
| 0. Remove deterministic builder | done | Codex | App Creator creates durable app-create tasks instead of writing app files directly. | Earlier commits: `e8bec3b8`, `b142e4c8`; installed path verified in later bench runs. |
| 1. Simplify skill/resources | in_progress | Codex | Skill is English, concise, reference/resource based, and avoids prompt walls. | Current skill/resources were simplified, but final entry-point proof is still open. |
| 2. Build CTOX-native bench | done | Codex | Bench submits real app-create tasks and records evidence without creating or repairing app files. | `ctox business-os app bench run/status`; run dirs under `runtime/business-os/app-creation-bench/`. |
| 3. Close lifecycle/orchestration gaps | in_progress | Codex | Queue, validation, launchd/dev-upgrade, module catalog, and native peer lifecycle work without manual service recovery. | `f009a3b4` and `bbbdbbd4` fixed earlier lifecycle issues; `rfix6` drained all five tasks without manual restart, but queue-drain latency remains a watch item. |
| 4. Close validator/resource gaps | done | Codex | Validator rejects known bad app artifacts before browser E2E finds them; skill resources state the same architecture expectations plainly. | `0dd04c31` rejects unscoped collections; `aa945a71` accepts and checks namespaced `data-*-action`; installed release `branch-main-20260621T014938Z`. |
| 5. Fresh five-app CTOX proof | done | Codex | One fresh post-validator run with five apps reaches terminal queue success and installed validation green. | `rfix6` snapshot `status-1782010063510.json`: handled=5, validation_passed=5, bench_green=true. |
| 6. Browser proof | blocked | Codex | Browser mount, UI persistence, reload persistence, and automation smoke pass for all five fresh apps. | `catalog-sync-diagnostic-1782010683224.json`: native/WebRTC bootstrap present, browser remains on `desktop` and does not see the bench app. |
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

Active phase: `6. Browser proof`

Current rule: do not patch generated app artifacts by hand. `rfix6` is the
active post-validator bench. Static installed validation is green; run browser
E2E against the generated runtime apps and classify any failures before patching
source, skill resources, validator, or smoke tooling.

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
  `runtime_orchestration_gap` candidate.
- [x] Observe Contracts and Quality lease without manual restart.
- [x] Confirm all five `rfix6` apps are handled and installed-validation green.
- [x] Run first fresh `rfix6` browser diagnostic.
- [x] Classify current browser blocker as `data_plane_gap` or module-catalog
  visibility gap candidate.
- [ ] Run a focused Playwright shell-state/RxDB probe that reads
  `window.CTOX_BUSINESS_OS_APP`, the browser `business_module_catalog` document,
  sync diagnostics, visible module ids, and governance founder keys.
- [ ] Patch only after the focused probe proves whether the issue is catalog
  replication, shell filtering/governance, or browser-smoke harness logic.
- [ ] After the catalog/mount blocker is fixed, run fresh `rfix6` browser E2E
  for mount, UI save, reload persistence, native sync, and automation command
  dispatch.
- [ ] Classify every browser E2E failure before patching.
- [ ] Update this file after every material result.

Current slice exit criteria:

- Browser catalog/mount blocker has a source-level root cause or a documented
  non-source explanation with evidence.
- Browser E2E evidence exists for all five `rfix6` apps after the mount blocker.
- Every `rfix6` browser failure is classified as app defect, smoke harness gap,
  data plane gap, validator gap, or skill/resource gap.
- If browser E2E is green, move to entry-point proof.

## Bench Matrix

Active run `rfix6`:

| Case | Module Id | Queue Task | Queue Status | Static Validation | Browser E2E | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix6` | `queue:system::b1d790469d5c9dae5978257f` | handled | green | blocked | Fresh browser shell with `#bench_subscriptions_rfix6` remains on `desktop`; root cause pending focused catalog probe. |
| Inventory | `bench_inventory_rfix6` | `queue:system::0443816ee994cd5b2a753272` | handled | green | blocked | Generated collections are module-scoped (`bench_inventory_rfix6_*`); browser app-specific E2E waits on the shared catalog/mount blocker. |
| Projects | `bench_projects_rfix6` | `queue:system::c46914ae3d7379543d66c59c` | handled | green | blocked | Completed after app-validation rework; app-specific E2E waits on the shared catalog/mount blocker. |
| Contracts | `bench_contracts_rfix6` | `queue:system::d3e25efcdcf4af03dac40faa` | handled | green | blocked | Initially failed module tests, then CTOX validation rework repaired it without manual artifact edits; app-specific E2E waits on the shared catalog/mount blocker. |
| Quality | `bench_quality_rfix6` | `queue:system::f67604fe551242397dd366d5` | handled | green | blocked | Initially had schema parity/test gaps, then CTOX validation rework repaired it without manual artifact edits; app-specific E2E waits on the shared catalog/mount blocker. |

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

1. Run a focused browser shell-state/RxDB probe for `rfix6`:
   `window.CTOX_BUSINESS_OS_APP`, browser `business_module_catalog`,
   `state.modules`, `state.governance`, sync diagnostics, and visible nav ids.
2. Compare browser catalog state with native
   `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os-rxdb.sqlite3`
   module catalog and founder governance.
3. Patch only the proven layer: data-plane replication, shell filtering, or
   browser-smoke logic. Do not patch generated app artifacts.
4. After the catalog/mount blocker is fixed, run fresh `rfix6` browser E2E with
   precise selectors and longer native/command sync waits.
5. Keep browser-smoke failures separate from generated app failures: record
   selector/wait gaps as harness issues, not app defects.
6. If a generated app fails browser E2E while static validation is green,
   classify whether this belongs in the validator, skill resources, runtime
   shell/data plane, or app-review rework behavior.
7. If browser E2E is green for all five apps, verify App Creator, Chat, App
   Store/template flow, CLI, and inbound/MCP entry paths all attach the same app
   module creation path.
8. Update this file before every handoff, before every source patch, and after
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
- `2026-06-21`: continued observation showed Contracts and Quality both leased without manual restart. Contracts initially failed module tests, then CTOX validation rework repaired it. Quality initially had schema parity/test gaps, then CTOX validation rework repaired it.
- `2026-06-21`: `rfix6` snapshot `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/status-1782010063510.json` shows `bench_green=true`, `handled=5`, `validation_passed=5`, `pending=0`, `failed=0`.
- `2026-06-21`: native RxDB catalog query against `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os-rxdb.sqlite3` shows all five `rfix6` module ids and all five `rfix6` founder governance keys in `business_module_catalog`.
- `2026-06-21`: browser diagnostic `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix6/browser-smoke/catalog-sync-diagnostic-1782010683224.json` opened `http://127.0.0.1:8765/?rxdbSmoke=1&smokeDbId=rfix6browser3#bench_subscriptions_rfix6`; final checkpoint stayed on `activeModule=desktop`, `hasBenchText=false`, `http_bridge_available=false`, `native_rxdb_peer_available=true`, and V1.5 fetch logs started for `business_module_catalog` and `ctox_runtime_settings`.

## Open Issues

- Queue-drain latency is still worth watching, but `rfix6` did finish all five
  app-create tasks without manual restart. Do not patch queue-drain unless a new
  run reproduces a terminal idle-with-pending stall.
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
- Fresh `rfix6` browser E2E is blocked by module-catalog visibility: native
  RxDB has all five modules and governance, but a fresh browser shell does not
  mount `#bench_subscriptions_rfix6` and remains on `desktop`.
- Focused browser state probing is required before a source patch. The next
  patch target is not yet proven.
- Verify App Creator, Chat, App Store/template, CLI, and inbound/MCP entry paths all use the same app-module skill/resource context.
- Confirm the skill remains short, English, resource-based, and not a prompt wall.
- Keep unrelated dirty file `tests/business-os/ats_synthetic_generate.sh` out of this work unless explicitly requested.
