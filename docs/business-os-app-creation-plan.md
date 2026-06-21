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

- Active install: `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260621T011143Z`
- Source head: `0dd04c31 Harden Business OS app collection validation`
- Upgrade path: latest `ctox upgrade --dev` applied the active release from
  `main`, but predates `0dd04c31`. Run `ctox upgrade --dev` again before using
  installed validation as evidence.
- Business OS shell assets are now byte-identical across source, state, and runtime:
  `src/apps/business-os/app.js`,
  `/Users/michaelwelsch/.local/state/ctox/business-os/app.js`, and
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app.js`
  all have SHA-256 `ec25ef4fd0ded5994c4aae5f529ad199e73b1da3857098da648807eae4d28ed7`.

Current proof run:

- Run id: `rfix5`
- Suite: `core-five`
- Model: `minimax-m3`
- Context: `256k`
- Entry path: real `ctox.business_os.app.create` tasks through installed CTOX.
- Evidence dir: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5`
- Latest static status snapshot: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1782001080447.json`
- Browser evidence:
  - `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/browser-smoke/subscriptions-canary-1782002040950.json`
  - `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/browser-smoke/browser-smoke-1782001975201.json`

Latest live result:

- Historical installed-validator snapshot for `rfix5` was green:
  `handled=5`, `validation_passed=5`, `bench_green=true`.
- Source validator has since been hardened locally to reject unscoped runtime
  collection names. Under that validator, historical `rfix5` apps such as
  `bench_inventory_rfix5` and `bench_projects_rfix5` are correctly red because
  they use shared/domain collection names.
- Installed module catalog contains all five `rfix5` modules as `source=installed`.
- Latest recorded `ctox status --json` showed `running=true`, `busy=false`,
  `manager=launchd-user`, Business OS web/MCP autostarted, native RxDB peer
  `replicationUp=true`, and `http_bridge_available=false`. Recheck `busy`
  before any `ctox upgrade --dev` or fresh bench run.
- Browser mount is proven for all five apps, but first mount from a fresh
  browser context can take roughly 25-47 seconds while the browser waits for the
  RxDB module catalog.
- Browser E2E is not production-green yet:
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
| Static validation | in_progress | Historical `rfix5` snapshot was green under the old validator. Source validator now rejects unscoped runtime collection names; install and fresh bench are pending. |
| Browser mount | done | All five apps mounted from installed runtime paths in fresh browser contexts with no console/page/request failures; mount latency remains an open quality issue. |
| Persistence | in_progress | Subscriptions, Projects, Contracts, and targeted Quality reached native SQLite; Inventory local reload works but native sync is blocked by unscoped collection-name drift. |
| Automation | in_progress | Subscriptions, Contracts, and targeted Projects created `business_os.chat.task`; Inventory must be reproven after scoped collections. |
| Entry-point coverage | pending | Chat, App Creator, App Store/template flow, CLI, and inbound/MCP paths all attach the same app-module skill/resource context. |
| Install/upgrade lifecycle | done | `ctox upgrade --dev` applied `branch-main-20260621T001015Z`; source/state/runtime shell assets match; service is running. |
| No regressions | in_progress | Static validation is green, but browser smoke is red and must block production signoff. |

## Phase Tracker

| Phase | Status | Owner | Exit Criteria | Evidence |
| --- | --- | --- | --- | --- |
| 0. Remove deterministic builder | done | Codex | App Creator creates durable app-create tasks instead of writing app files directly. | Earlier commits: `e8bec3b8`, `b142e4c8`; installed path verified in later bench runs. |
| 1. Simplify skill/resources | in_progress | Codex | Skill is English, concise, reference/resource based, and avoids prompt walls. | Current skill/resources were simplified, but final entry-point proof is still open. |
| 2. Build CTOX-native bench | done | Codex | Bench submits real app-create tasks and records evidence without creating or repairing app files. | `ctox business-os app bench run/status`; run dirs under `runtime/business-os/app-creation-bench/`. |
| 3. Close lifecycle/orchestration gaps | done | Codex | Queue, validation, launchd/dev-upgrade, module catalog, and native peer lifecycle work without manual service recovery. | `f009a3b4` and `bbbdbbd4`; `ctox upgrade --dev` applied `branch-main-20260621T001015Z`; service running with native RxDB peer `replicationUp=true`. |
| 4. Close validator/resource gaps | in_progress | Codex | Validator rejects known bad app artifacts before browser E2E finds them; skill resources state the same architecture expectations plainly. | Local patch rejects unscoped runtime collection names; tests pass. Install and fresh bench pending. |
| 5. Fresh five-app CTOX proof | done | Codex | One fresh run with five apps reaches terminal queue success and installed validation green. | `rfix5`: 5/5 handled, 5/5 installed-validation green, `bench_green=true`. |
| 6. Browser proof | in_progress | Codex | Browser mount, UI persistence, reload persistence, and automation smoke pass for all five fresh apps. | Mount 5/5; full E2E 2/5 green: Subscriptions and Contracts. |
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

Active phase: `4. Close validator/resource gaps`

Current rule: do not patch generated app artifacts by hand. The module-scoped
collection validator/resource update is pushed to `main`; it must now be
installed with `ctox upgrade --dev` and validated from the installed CTOX path
before a fresh bench is meaningful.

Immediate checklist:

- [x] Confirm installed release is `branch-main-20260621T001015Z`.
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
- [ ] Run `ctox upgrade --dev` after the patch is on `main`.
- [ ] Verify the installed validator rejects historical unscoped `rfix5`
  modules.
- [ ] Run a fresh CTOX five-app bench after the systemic fixes.
- [ ] Repeat browser E2E until all five apps are green.
- [ ] Update this file after every material result.

Current slice exit criteria:

- Inventory, Projects, and Quality failures each have one failure class.
- The next source/skill/validator/runtime patch target is named explicitly.
- If no patch is justified, the plan says why and identifies the next fresh
  bench command instead.

## Bench Matrix

Active run `rfix5`:

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

1. Run `ctox upgrade --dev` now that `main` contains `0dd04c31`.
2. Verify the installed validator contains the module-scoped collection check
   and rejects historical unscoped `rfix5` modules.
3. Run a fresh five-app CTOX bench, for example `rfix6`, after the upgrade.
4. Confirm newly generated runtime collection names are module-id scoped.
5. Run browser E2E with precise selectors and longer native/command sync waits.
6. Keep browser-smoke failures separate from app failures: record selector/wait
   gaps as harness issues, not generated app defects.
7. Update this file before handing off and after every material result.

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
- `2026-06-21`: commit `0dd04c31` pushed to `main` with the app-creation plan, module-scoped collection validator, validator regression test, and matching skill resource updates. Next proof step is `ctox upgrade --dev`.

## Open Issues

- Historical `rfix5` artifacts are invalid under the hardened source validator
  because runtime collections are not module-id scoped.
- Browser E2E for Inventory remains red until a fresh post-validator app uses
  scoped collections and proves native sync.
- Fresh-browser mount works but takes 25-47 seconds in observed cases; decide whether that is acceptable or a shell catalog readiness issue.
- Repeated bench runs produced domain-level collection names such as
  `bench_inventory_items`; this is classified as `skill_resource_gap` plus
  `validator_gap` and patched locally.
- Browser-smoke selector/wait precision is weak: Projects needed a scoped detail
  button selector and command replication wait; Quality needed a longer native
  sync wait.
- Verify App Creator, Chat, App Store/template, CLI, and inbound/MCP entry paths all use the same app-module skill/resource context.
- Confirm the skill remains short, English, resource-based, and not a prompt wall.
- Keep unrelated dirty file `tests/business-os/ats_synthetic_generate.sh` out of this work unless explicitly requested.
