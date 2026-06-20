# Business OS App Creation Plan

Purpose: make CTOX Business OS app creation reliable through the real CTOX
Business OS paths. A user should be able to ask CTOX for a small business app
and receive a runtime-installed, immediately runnable vanilla HTML/CSS/browser
ESM app that persists data through CTOX DB and can dispatch normal CTOX
Business OS automation commands.

This is a working plan. Update it during execution, not only at handoff.

## Update Protocol

Every agent that works on app creation must update this file before handing off.

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
- Mark a failure class before patching anything.
- Do not use this plan as an app-building prompt.
- Do not expand this file into a long prompt or rule wall. Keep it operational.

Status values: `pending`, `in_progress`, `blocked`, `done`.

## Current State

Last updated: `2026-06-21`

Overall status: `in_progress`, not production-ready.

Current CTOX installed release:

- Active install: `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T220404Z`
- Source head: `3ce6863d Fix app validation rework completion`
- Important source state: commit `3ce6863d` is pushed but not installed yet.

Current proof run:

- Run id: `rfix5`
- Suite: `core-five`
- Model: `minimax-m3`
- Context: `256k`
- Entry path: real `ctox.business_os.app.create` tasks through installed CTOX.
- Evidence dir: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5`
- Latest status snapshot: `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781998625164.json`

Latest live result:

- `handled=4`, `leased=1`, `failed=0`, `blocked=0`.
- `validation_passed=4`, `validation_failed=1`.
- Subscriptions, Projects, Contracts, and Quality are handled and installed-validation green.
- Inventory is still leased by `ctox-service`; its artifacts are complete but its module tests currently fail 3/68. Do not classify it terminal until CTOX finishes the worker or moves it into validation rework.
- `ctox status --json` reports `running=true`, `manager=launchd-user`, `busy=true`, `worker_active_count=1`, Business OS web/MCP autostarted, and native RxDB peer `replicationUp=true`.

Do not run `ctox upgrade --dev` while the active Inventory worker is running.

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
| Correct install location | in_progress | Generated apps land only in runtime installed-module directories on a normal CTOX install. |
| CTOX-native creation | in_progress | Five bench apps are created through real Business OS app-create tasks, not direct file writes. |
| Static validation | in_progress | Required files, `node --check`, module tests, schema parity, record-helper type checks, no known UI/runtime blockers. |
| Browser mount | pending | Each fresh app opens in Business OS with no console/page/request failures. |
| Persistence | pending | Each app creates/edits one record through UI, reloads, and proves the record exists via native CTOX DB sync. |
| Automation | pending | Each app dispatches one valid `business_os.chat.task` or allowed ticket command through `ctx.commandBus.dispatch`. |
| Entry-point coverage | pending | Chat, App Creator, App Store/template flow, CLI, and inbound/MCP paths all attach the same app-module skill/resource context. |
| Install/upgrade lifecycle | in_progress | Source fixes install with `ctox upgrade --dev`; service resumes under launchd/systemd/process manager with no manual recovery. |
| No regressions | pending | Relevant Rust/Node tests and browser smoke pass on the installed release. |

## Phase Tracker

| Phase | Status | Owner | Exit Criteria | Evidence |
| --- | --- | --- | --- | --- |
| 0. Remove deterministic builder | done | Codex | App Creator creates durable app-create tasks instead of writing app files directly. | Earlier commits: `e8bec3b8`, `b142e4c8`; installed path verified in later bench runs. |
| 1. Simplify skill/resources | in_progress | Codex | Skill is English, concise, reference/resource based, and avoids prompt walls. | Current skill/resources were simplified, but final entry-point proof is still open. |
| 2. Build CTOX-native bench | done | Codex | Bench submits real app-create tasks and records evidence without creating or repairing app files. | `ctox business-os app bench run/status`; run dirs under `runtime/business-os/app-creation-bench/`. |
| 3. Close lifecycle/orchestration gaps | in_progress | Codex | Queue, validation, launchd/dev-upgrade, module catalog, and native peer lifecycle work without manual service recovery. | Multiple fixes landed; `3ce6863d` still needs installed proof. |
| 4. Close validator/resource gaps | in_progress | Codex | Validator rejects known bad app artifacts before browser E2E finds them; skill resources state the same architecture expectations plainly. | Schema parity validator installed in `branch-main-20260620T220404Z`; command-bus alias validator in `3ce6863d` not installed yet. |
| 5. Fresh five-app CTOX proof | in_progress | Codex | One fresh run with five apps reaches terminal queue success and installed validation green. | Active run `rfix5`: 4/5 handled green, Inventory leased. |
| 6. Browser proof | pending |  | Browser mount, UI persistence, reload persistence, and automation smoke pass for all five fresh apps. | Must run after `rfix5` is terminal green or after the next fresh run if `rfix5` is superseded. |
| 7. Entry-point proof | pending |  | Every user-facing app creation/modification path uses the same skill/resource context and runtime app contract. | Not done. |
| 8. Production signoff | pending |  | All production gates are green, latest source is installed, plan and docs updated, no unrelated dirty files staged. | Not done. |

## Active Slice

Owner: `Codex`

Active phase: `5. Fresh five-app CTOX proof`

Current rule: wait for Inventory to finish or enter validation rework before
installing the pushed source patch.

Immediate checklist:

- [x] Confirm installed release is `branch-main-20260620T220404Z`.
- [x] Confirm `rfix5` was submitted through real app-create tasks.
- [x] Confirm four apps are handled and installed-validation green.
- [x] Confirm Inventory worker is still active.
- [ ] Wait until `ctox status --json` reports no active worker or Inventory reaches terminal/rework status.
- [ ] If idle, install source head through `CTOX_INSTALL_ROOT=/Users/michaelwelsch/.local/lib/ctox cargo run --bin ctox -- upgrade --dev`.
- [ ] Rerun `ctox business-os app bench status --run-id rfix5 --validate --json`.
- [ ] If all five apps are terminal validation-green, run browser mount smoke.
- [ ] If browser mount is green, run `ctx.db` persistence smoke.
- [ ] If persistence is green, run `ctx.commandBus.dispatch` automation smoke.
- [ ] If any gate fails, classify the failure before patching.
- [ ] Update this file after every material result.

## Bench Matrix

Active run `rfix5`:

| Case | Module Id | Queue Task | Queue Status | Validation | Notes |
| --- | --- | --- | --- | --- | --- |
| Subscriptions | `bench_subscriptions_rfix5` | `queue:system::1f1fc12323db6d76c2d82b4f` | handled | green, 29/29 tests | Ready for browser smoke after all five apps are terminal. |
| Inventory | `bench_inventory_rfix5` | `queue:system::ba7b80e7822eba234b516731` | leased | red while worker active, 65/68 tests | Do not classify until worker completes or rework is leased. |
| Projects | `bench_projects_rfix5` | `queue:system::0a6a72bdb295789065e82cf0` | handled | green, 29/29 tests | Ready for browser smoke after all five apps are terminal. |
| Contracts | `bench_contracts_rfix5` | `queue:system::265eb0e3dce584b5352ae416` | handled | green, 48/48 tests | Exposed green-rework completion gap; source fixed in `3ce6863d`, install pending. |
| Quality | `bench_quality_rfix5` | `queue:system::19327b78a8dda35d19ce3cea` | handled | green, 23/23 tests | Ready for browser smoke after all five apps are terminal. |

Historical runs:

- `rcli`: exposed private app lifecycle/visibility gap and reference mismatch.
- `rfix1`: exposed source-reference catalog gap.
- `rfix2`: proved dynamic runtime collection registration, then exposed browser E2E validator gaps.
- `rfix3`: exposed duplicate runtime functions and missing Save/Submit controls.
- `rfix4`: exposed schema/record parity drift; now correctly red under the installed validator.
- `rfix5`: active proof run after schema/record parity validation.

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

1. Keep polling `ctox status --json` and `ctox business-os app bench status --run-id rfix5 --validate --json`.
2. Do not interrupt the active Inventory worker.
3. When the worker is idle, install source head `3ce6863d` with:

   ```sh
   CTOX_INSTALL_ROOT=/Users/michaelwelsch/.local/lib/ctox cargo run --bin ctox -- upgrade --dev
   ```

4. Recheck the active release symlink and CTOX service status.
5. Continue `rfix5` if it can become terminal-green; otherwise classify the Inventory failure.
6. Run browser mount, persistence, and automation smoke only on a fresh terminal-green run.
7. Update this file before committing or handing off.

## Evidence Log

- `2026-06-20`: schema/record parity validator landed in commit `ebfba103` and was installed as `branch-main-20260620T220404Z`. Historical `rfix4` is correctly red under that validator.
- `2026-06-20`: fresh run `rfix5` started through installed CTOX with `minimax-m3`, `256k`, and five real app-create tasks.
- `2026-06-20`: commit `3ce6863d` fixed green app-validation rework completion in source and expanded command-bus alias validation. Verification passed: `node src/apps/business-os/scripts/validate-app-module.test.mjs`, `cargo test --bin ctox business_os_app_validation_`, `cargo test --bin ctox worker_finalization_`, targeted rustfmt/diff checks, and `cargo check --bin ctox`.
- `2026-06-21`: live status snapshot `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix5/status-1781998625164.json` shows `rfix5` at 4/5 handled and installed-validation green while Inventory is still leased.

## Open Issues

- Install and prove source head `3ce6863d` after the active worker is idle.
- Complete `rfix5` or supersede it with a fresh post-install run if Inventory ends red.
- Add or run browser mount smoke for the latest terminal-green five-app run.
- Add or run `ctx.db` persistence smoke for the latest browser-green five-app run.
- Add or run `ctx.commandBus.dispatch` automation smoke for the latest persistence-green five-app run.
- Verify App Creator, Chat, App Store/template, CLI, and inbound/MCP entry paths all use the same app-module skill/resource context.
- Confirm the skill remains short, English, resource-based, and not a prompt wall.
- Keep unrelated dirty file `tests/business-os/ats_synthetic_generate.sh` out of this work unless explicitly requested.
