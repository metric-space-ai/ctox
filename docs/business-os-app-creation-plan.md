# Business OS App Creation Plan

Purpose: make CTOX Business OS app creation reliable through the real Business
OS entry points. CTOX should build and modify small runnable Business OS apps as
runtime-installed vanilla HTML/CSS/browser ESM modules, using the Business OS
app-module skill resources and normal CTOX validation.

This is an editable work plan. Agents working on this must update the tracker,
phase notes, evidence log, and open issues in this file before handing off.

## How To Use This Plan

This file is the running project ledger for Business OS app creation. Every
agent that works on app creation must edit this file during the work, not only
at the end.

Update rules:

- Move exactly one active phase to `in_progress` before starting material work.
- Add owner, date, and concrete evidence for every phase update.
- Append an Evidence Log entry after every meaningful run, failed run, patch,
  validator change, or release-install test.
- Add blockers to Open Issues immediately when work cannot continue.
- Mark a phase `done` only when its exit criteria are met with evidence.
- Keep notes factual: command, commit, path, run id, task id, validator output,
  browser-smoke result, or failure classification.
- Keep the Current Execution Slice below current. It is the first thing a
  continuation agent should read and the last thing it should update.

Do not use this plan as an app-building prompt. CTOX must still build apps
through normal agent execution, the Business OS app-module skill resources, and
the Business OS command/task pipeline.

Editable sections:

- Current Status
- Current Execution Slice
- Tracker
- Phase update checklists
- Evidence Log
- Open Issues
- Handoff Notes

Do not edit stable sections such as Work Policy, Non-Negotiables, Acceptance
Gates, or Failure Classification unless the Evidence Log shows that the current
rule is wrong or incomplete.

## Current Status

Status: `in_progress`

Last updated: `2026-06-20`

Current baseline:

- The former direct-file writer in App Creator has been removed.
- The App Creator now creates `ctox.business_os.app.create` tasks instead of
  writing app files directly.
- The `business-os-app-module-development` skill is a concise English resource
  index, not a task script.
- Skill resources are English and cover module contract, Do/Don't list, green
  checklist, and architecture translation.
- `ctox upgrade --dev` installed release `branch-main-20260620T120455Z`
  from current main after these changes.
- `ctox upgrade --dev` then installed release `branch-main-20260620T124515Z`
  with the lifecycle fix from commit `212aa2d0`.
- `ctox upgrade --dev` then installed release `branch-main-20260620T130820Z`
  with the reference-catalog and skill-resource fix from commit `c1267d0d`.
- `ctox upgrade --dev` then installed release `branch-main-20260620T141728Z`
  with the runtime module-id finalization fix from commit `b7632c6a` and the
  bench CLI help guard.
- `ctox business-os app bench run` now submits the core-five app-create bench
  through the normal Business OS command path and writes JSONL evidence under
  `runtime/business-os/app-creation-bench/`.
- `ctox business-os app bench status --run-id rcli --validate` now writes a
  read-only status snapshot and distinguishes status collection from a green
  bench.
- Browser smoke against a validation-green `rcli` app exposed a real runtime
  lifecycle gap: private 0.x installed apps were projected into
  `business_module_catalog`, but lacked creator/responsible assignment and an
  initial app-version snapshot, so they were not openable by the local Business
  OS user.
- A source patch now makes app-create validation success seed the creating actor
  as founder/responsible and records the initial `app_create` module version
  before catalog projection. The bench runner also defaults to the local
  Business OS session user instead of an artificial `rxdb-command` actor when
  no actor is supplied.
- App creation is not yet production-ready until a fresh CTOX-native bench run
  passes end to end on the installed release path.
- Fresh run `rfix1` proves the bench commands now default to actor `local-dev`,
  but it also exposed a `reference_gap`: the current installed reference
  catalog shows source app manifest fields that the runtime validator rejects
  for generated apps, including `layout.icon_svg`, `store.installable`, and
  unqualified `layout.right`.
- The installed reference catalog now emits explicit runtime rules, marks
  recommended business-workflow references, and warns that source-only manifest
  fields must not be copied into runtime-installed apps.
- App creation is still not production-ready. The active post-reference run
  `rfix2` now has two terminal-green apps: Inventory and Projects are both
  `handled`, pass installed validation, and have complete runtime-installed
  artifacts. Quality is stuck in validation rework with no artifact directory,
  and Subscriptions plus Contracts are pending while the service is idle.
  Source forensics found a queue rework-recognition bug: rework prompts now
  begin with `Business OS app validation failed.`, but the dispatcher only
  matched the older `Business OS app artifact validation failed.` marker. The
  source patch accepts both markers and is verified by a targeted regression
  test, but it is not installed yet. The next proof must be one fresh five-app
  CTOX-native bench with validation, browser smoke, persistence, and automation
  evidence for all five apps.
- The Inventory finalization bug was a concrete runtime lifecycle bug: direct
  validation previously succeeded on files but failed finalization because
  app-version snapshot recording slugified `bench_inventory_rfix2` to
  `bench-inventory-rfix2`. Release `branch-main-20260620T141728Z` fixes this
  on the installed path.

## Current Execution Slice

Owner: `Codex`

Started: `2026-06-20`

Active phase: `5. Repeat bench after installed reference fix`

Objective: produce a fresh five-app CTOX-native bench after the installed
reference-catalog/skill-resource fix, then classify remaining failures from
evidence before changing code or resources. The existing `rfix1` run remains
useful lifecycle and reference-gap evidence, but it was started before the
reference-catalog patch was installed and must not be treated as green proof.

Immediate checklist:

- [x] Collect queue status for all five `rcli` task ids.
- [x] Record every produced module path under
      `runtime/business-os/installed-modules/`.
- [x] Run installed validation only for apps with module artifacts.
- [x] Record missing files and validator failures per app.
- [x] Run first browser smoke against a validation-green installed app.
- [x] Classify the browser visibility failure as `runtime_orchestration_gap`.
- [x] Patch app-create validation success to seed lifecycle assignment and an
      initial app version for runtime-installed modules.
- [x] Patch bench default actor to use the local Business OS session user.
- [x] Verify the patched code with narrow Rust tests.
- [x] Run `git diff --check` for changed files.
- [x] Commit and push the systemic lifecycle fix.
- [x] Install lifecycle fix with `ctox upgrade --dev`.
- [x] Start a fresh five-app bench without `--actor` so it uses the local
      Business OS user.
- [x] Confirm fresh bench command actor is `local-dev`.
- [x] Classify first `rfix1` architecture failure as `reference_gap`.
- [x] Patch the reference catalog to avoid presenting source-only manifest
      fields as runtime app templates.
- [x] Patch skill resources with the same source-reference adaptation rule.
- [x] Verify the reference-catalog patch with a regression test.
- [x] Commit and push the reference-catalog/skill-resource fix.
- [x] Install reference fix with `ctox upgrade --dev`.
- [x] Verify installed reference output exposes runtime rules and source-field
      warnings.
- [x] Let any pre-patch queued bench tasks finish or explicitly record why they
      were superseded.
- [x] Start a fresh five-app bench after the reference fix is installed.
- [x] Confirm fresh bench command actors are `local-dev`.
- [x] Capture current `rfix2` status snapshot and record that only Inventory
      has validation-green artifacts so far.
- [ ] Collect terminal installed validation status for all five fresh bench
      apps.
- [ ] Record whether any app dispatched a real automation command.
- [ ] Run browser smoke after the fresh bench has validation-green artifacts.
- [ ] Classify every remaining failure before editing code or skill resources.
- [x] Classify the Inventory finalization failure as runtime lifecycle
      ID-normalization bug, not app-output failure.
- [x] Patch runtime app-version snapshotting to preserve underscore module ids.
- [x] Commit, push, and install the underscore module-id finalization fix via
      `ctox upgrade --dev`.
- [x] Re-run direct validation/finalization for `bench_inventory_rfix2` after
      the installed fix.
- [x] Decide whether remaining idle pending `rfix2` tasks are a worker
      scheduling gap, normal queue cadence, or require superseding/retry.
- [x] Let the currently leased `bench_projects_rfix2` worker reach terminal
      evidence before patching anything else.
- [x] Recheck `bench_quality_rfix2` rework after the active worker finishes.
- [x] Recheck pending Subscriptions and Contracts after the active worker
      finishes.
- [x] Classify current failures before patching code, skill resources, or
      validation.
- [x] Patch source rework detection to recognize the current validation
      feedback header and the legacy artifact-validation marker.
- [x] Verify rework priority with
      `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`.
- [ ] Commit and push the rework-detection patch and this plan update.
- [ ] Install the patch via `ctox upgrade --dev`.
- [ ] Cleanup or cancel only superseded old-run `rcli` validation-rework tasks
      before letting the installed dispatcher resume `rfix2`.
- [ ] Verify the installed service leases `bench_quality_rfix2` rework or one
      of the pending `rfix2` tasks instead of idling.
- [ ] Collect terminal installed validation status for all five fresh bench
      apps.
- [x] Update Evidence Log and Open Issues before handoff.

Do not patch the app outputs directly. Do not add deterministic builders. Do not
add skill rules for the old `rcli` project-app helper-test failure unless the
same class repeats. The lifecycle patch was allowed because browser smoke proved
a load-bearing Business OS lifecycle/orchestration gap. The current reference
patch is allowed because `rfix1` copied source-only manifest patterns exposed by
the reference catalog. The current rework-detection patch is allowed because
the dispatcher skipped durable validation-rework tasks even though their
Business OS app module ids and validator reports were present.

## Tracker

| Phase | Status | Owner | Evidence | Notes |
| --- | --- | --- | --- | --- |
| 0. Remove wrong architecture | done | Codex | `e8bec3b8`, `b142e4c8`, installed release `branch-main-20260620T102259Z` | App Creator no longer writes app files itself; resource-index skill installed. |
| 1. Define acceptance gates | pending |  |  | Formalize what must pass for app creation, modification, validation, browser smoke, and automation. |
| 2. Build CTOX-native bench runner | done | Codex | `8a8cd236`; `cargo test --bin ctox app_bench_`; installed release `branch-main-20260620T113510Z`; CLI run `rcli` | Runner submits real `ctox.business_os.app.create` tasks, writes runtime JSONL evidence, and does not write app artifacts. |
| 3. Run five-app bench in CTOX | blocked | Codex | run `rcli`; installed status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rcli/status-1781958189008.json`; browser smoke against `http://127.0.0.1:8765/#bench_subscriptions_rcli` | `rcli` produced two validation-green apps, but browser smoke showed validation-green private apps were not openable because creator/responsible lifecycle fields were empty. Superseded by Phase 4 fixes; continue with a fresh post-fix run in Phase 5. |
| 4. Patch systemic gaps | done | Codex | lifecycle commit `212aa2d0`; reference commit `c1267d0d`; installed releases `branch-main-20260620T124515Z` and `branch-main-20260620T130820Z`; run `rfix1`; `cargo test --bin ctox app_bench_`; `cargo test --bin ctox app_validation_success_accepts_postlease_artifact_write`; `cargo test --bin ctox app_references_mark_source_only_manifest_fields_as_non_templates`; `ctox business-os app references --json` | Classification from `rcli`: project helper-test mismatch is `model_failure`; private app visibility is `runtime_orchestration_gap`. Classification from `rfix1`: raw source reference metadata is `reference_gap`. Patched only lifecycle/orchestration and reference-resource gaps. No app-output repair and no deterministic builder. |
| 5. Repeat until green | in_progress | Codex | installed releases `branch-main-20260620T130820Z` and `branch-main-20260620T141728Z`; `ctox status`; `ctox business-os app references --json`; `ctox queue cleanup-scope --match-run-id rfix1 --cancel-open`; `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix2`; latest status `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781966787521.json`; direct validation `ctox business-os app validate bench_inventory_rfix2 --installed`; source tests `cargo test --bin ctox app_validation_success_`, `cargo test --bin ctox business_os_app_validation_worker_error_after_green_completes_business_command`, `cargo test --bin ctox app_bench_`, `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks` | Pre-reference `rfix1` tasks were cancelled as superseded; accidental run `r1781961729513` was cancelled after `bench run --help` unexpectedly submitted a task. Fresh `rfix2` uses actor `local-dev`. Inventory and Projects are terminal-green and `handled`. Quality is in no-artifact validation rework; Subscriptions and Contracts are pending; the service is idle because installed dispatch does not recognize the current rework feedback marker. Source patch fixes the marker mismatch but is not installed yet. No five-app green proof yet. |
| 6. Entry point coverage | pending |  |  | Verify App Creator, Chat, App Store/template flow, CLI, and external inbound paths bind the same skill/resource context. |
| 7. Production signoff | pending |  |  | All entry points produce runnable validated apps with evidence. |

Status values: `pending`, `in_progress`, `blocked`, `done`.

## Work Policy

The goal is a simple and robust app-creation path, not a deterministic app
generator.

Allowed work:

- Improve the Business OS app-module skill resources when repeated evidence
  shows that agents miss a CTOX-specific architecture concept.
- Improve validation when bad app artifacts are accepted.
- Improve CTOX task orchestration when app creation, review, validation,
  rework, or evidence collection is not durable.
- Add CLI commands that submit, inspect, validate, or benchmark real CTOX app
  tasks.
- Add tests and smoke checks that prove apps created through CTOX run on the
  release install path.

Forbidden work:

- Do not add a deterministic app generator that writes the app files itself.
- Do not add hidden templates that pretend to be agent-created apps.
- Do not repair bench apps inside the bench runner.
- Do not make source-checkout assumptions for runtime-created apps.
- Do not add long prompt templates to the skill or App Creator.
- Do not hide failures with validator bypasses, legacy exceptions, or fallback
  data paths.
- Do not expand the rule set for one-off model oddities unless the same failure
  repeats across runs or exposes a real architecture gap.

## Non-Negotiables

Do:

- Create apps under the installed CTOX runtime app directory:
  `runtime/business-os/installed-modules/<module-id>/`.
- Use source module paths only for checked-in store/template modules.
- Build apps as plain HTML fragments, CSS, and browser ESM.
- Use local browser ESM only. No package manager and no build step.
- Persist app records through the shell-provided `ctx.db` handle.
- Trigger intelligent workflows through `ctx.commandBus.dispatch(...)`.
- Use `business_os.chat.task` for normal CTOX follow-up automation with
  `payload.record_snapshot`.
- Use `ctox.ticket.local.*` only for real ticket lifecycle actions.
- Require `business-os-app-module-development` for create and modify tasks.
- Inspect three relevant shipped apps before implementation.
- Run `ctox business-os app validate <module-id> --installed`.

Do not:

- Do not reintroduce direct app-file builders or file templates that pretend to
  build the app.
- Do not add task scripts disguised as skill text.
- Do not write classic task text into the skill.
- Do not put user-created runtime apps under `src/`.
- Do not add React, Next.js, Vite, bundled dependencies, `node_modules`, or
  package-manager workflow.
- Do not create HTTP, REST, IndexedDB, SQLite, Postgres, `localStorage`, or
  `sessionStorage` persistence paths for app records.
- Do not write directly to `business_commands` or `ctox_ticket_*` projection
  collections from app code.
- Do not add dead controls, decorative third panes, fake AI buttons, or broad
  features that do not work.
- Do not mark an app task complete when validation is red.
- Do not hide failures by weakening validation.

## Target Entry Points

All of these must bind the same app-module skill resources and create the same
kind of Business OS app task:

- Business OS App Creator
- Business OS Chat
- Business OS App Store app creation or template flow
- CTOX CLI inbound app creation command
- External Business OS MCP or inbound communication path that asks CTOX to build
  or modify an app

Each entry point must pass module id, requested app description, install target,
mode, desired version, and required skill metadata as structured task context.

The app task should carry structured context, not a classic long prompt. It may
include the user request, target module id, install target, version intent,
required skill id, and validation expectations. The skill and references carry
the implementation rules.

## Acceptance Gates

An app creation task is green only when all gates pass:

1. Files exist only under the expected app target directory.
2. `module.json` uses installed-module semantics:
   `entry: installed-modules/<module-id>/index.html`,
   `install_scope: installed`, and semantic version such as `0.1.0`.
3. Required files exist:
   `module.json`, `collections.schema.json`, `schema.js`, `index.html`,
   `index.css`, `index.js`, `icon.svg`, local ESM helpers as needed,
   `locales/en.json`, `locales/de.json`, and tests.
4. `index.html` is an HTML fragment, not a full HTML document.
5. `index.js` exports `mount(ctx)` and renders into `ctx.host`.
6. Records persist via `ctx.db` and declared module collections.
7. At least one automation action dispatches a valid Business OS command.
8. The UI has no dead buttons and no unnecessary third pane.
9. Pure helper tests pass.
10. `ctox business-os app validate <module-id> --installed` passes.
11. Browser smoke opens the app, creates or edits one record, reloads, and sees
    persisted state.
12. Automation smoke dispatches one `business_os.chat.task` or valid local
    ticket command.

## Bench Suite

The bench intentionally uses simple user-level app requests. Do not
overspecify UI layout or implementation details. The agent must select three
reference apps itself.

| Bench App | Minimum Business Scope | Required Automation |
| --- | --- | --- |
| Subscriptions | Abo contracts, MRR, renewal date, churn risk | Create CTOX follow-up for renewal or churn-risk review. |
| Inventory | Items, stock locations, minimum stock, stock movement | Create CTOX follow-up for low-stock review. |
| Projects | Time/material vs fixed-price, milestones, budget vs actual | Create CTOX follow-up for over-budget or overdue milestone. |
| Contracts | Customer contracts, SLA, renewal, termination window | Create CTOX follow-up for renewal or cancellation deadline. |
| Quality | Complaints, corrective actions, audits, owner, due date | Create CTOX follow-up or local ticket for compliance action. |

## Bench Runner Requirements

Create a CTOX-native runner that:

- Removes old bench apps before each run.
- Creates five real `ctox.business_os.app.create` tasks.
- Uses MiniMax M3 through CTOX with 256k context.
- Waits for task completion or validation rework.
- Runs static validation for each produced app.
- Runs browser smoke for each produced app.
- Captures worker events, file lists, validation reports, browser console
  errors, and command dispatch evidence.
- Writes a compact JSONL evidence log under `runtime/` or another ignored
  runtime evidence directory.
- Never builds app files itself.
- Never repairs generated apps directly inside the runner.

Suggested command shape:

```text
ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k
```

The command shape is a suggestion for operator ergonomics, not a builder
contract.

The runner is only an evidence and orchestration tool. It must prove what CTOX
and the selected coding model do; it must not improve app output by writing or
rewriting app artifacts.

## Failure Classification

Each failed run must be classified before changing code:

- `skill_resource_gap`: the resource files omit a necessary Business OS concept.
- `validator_gap`: invalid app artifacts were accepted.
- `runtime_orchestration_gap`: CTOX queue, validation rework, completion, or
  provider handling broke despite valid/invalid app evidence being clear.
- `entrypoint_gap`: a route did not bind skill resources or did not create a
  structured app task.
- `model_failure`: one-off failure despite resources, validator, and runtime
  behaving correctly.
- `reference_gap`: reference app selection exposed bad examples or internal
  tools as app templates.

Patch only systemic failures. Do not add narrow rules for a single odd run.

## Phase Details

### Phase 1: Acceptance Gates

Status: `pending`

Tasks:

- Audit current validator coverage against the gates above.
- Decide how internal shell tools such as App Creator are excluded from normal
  app validation or validated with an explicit internal-tool mode.
- Add or update tests for installed-module manifest semantics, no source-path
  runtime output, no package manager, no local persistence, no dead controls
  where statically detectable, and command-bus automation shape.

Exit criteria:

- The validator rejects known bad artifacts and accepts known good installed
  runtime apps.
- Internal shell tools do not pollute normal generated-app validation evidence.

Phase update checklist:

- [ ] Validator audit recorded in Evidence Log.
- [ ] Known-bad fixtures or tests added where gaps exist.
- [ ] Known-good installed runtime app still validates.
- [ ] No validator bypass added for generated apps.

### Phase 2: CTOX-Native Bench Runner

Status: `done`

Tasks:

- Add the bench runner command.
- Make it submit real Business OS app-create tasks.
- Make it collect evidence without modifying app output.
- Make cleanup remove only bench apps by prefix/tag.

Exit criteria:

- A controlled run can submit tasks and collect evidence.
- A stopped run leaves enough evidence to explain what happened.
- The runner does not create, edit, or repair app files.

Phase update checklist:

- [x] CLI command or equivalent CTOX-native runner added.
- [x] Runner submits real `ctox.business_os.app.create` tasks.
- [x] Evidence path under ignored `runtime/` documented.
- [x] Cleanup only touches bench-tagged runtime apps.
- [x] Tests prove the runner does not write app artifacts.

### Phase 3: First Five-App CTOX Run

Status: `blocked`

Tasks:

- Run all five bench apps with MiniMax M3 through CTOX.
- Record produced paths, task IDs, validation result, browser smoke result, and
  automation smoke result.
- Stop after systemic failure if continuing would only create duplicate noise.

Exit criteria:

- Every failure has an evidence-backed classification.

Phase update checklist:

- [x] Run id recorded.
- [x] Five queue task ids recorded.
- [x] Produced module paths recorded for all apps that currently have
      artifacts.
- [x] Validation results recorded for all apps that currently have artifacts.
- [x] Initial browser smoke recorded for validation-green app.
- [ ] Automation dispatch evidence recorded per app.
- [x] Current failures classified before any patch.

### Phase 4: Systemic Fixes

Status: `done`

Tasks:

- Patch the smallest architecture-level cause for each majority failure.
- Prefer resource clarification, validator coverage, or runtime orchestration
  fixes over additional narrow rules.
- Keep the skill concise.
- Keep app creation agent-driven.

Exit criteria:

- Each patch maps to a repeated failure class or a clearly load-bearing
  architecture gap.

Phase update checklist:

- [x] Failure class named before patching.
- [x] Patch scope limited to skill resource, validator, entry point, or
      orchestration gap.
- [x] No app-specific bench repair committed.
- [x] Regression test or evidence added.
- [x] Lifecycle patched tree installed through `ctox upgrade --dev`.
- [x] Fresh CTOX-native bench started after lifecycle install.
- [x] Reference-catalog patched tree installed through `ctox upgrade --dev`.
- [x] Installed reference output inspected after upgrade.

### Phase 5: Repeat Bench

Status: `in_progress`

Tasks:

- Delete generated bench apps.
- Rerun the five-app suite.
- Compare failures against previous round.
- Continue until all five apps pass validation, browser smoke, persistence, and
  automation smoke in one CTOX-native run.

Exit criteria:

- One clean five-app run from a fresh bench root.

Phase update checklist:

- [ ] Previous bench apps removed by bench tag/prefix only.
- [ ] Fresh five-app run completed.
- [ ] Results compared with prior run.
- [ ] Remaining failures classified.

### Phase 6: Entry Point Coverage

Status: `pending`

Tasks:

- Run at least one app creation through each required entry point.
- Verify every path binds the same skill resources and target metadata.
- Verify no route writes app files directly.

Exit criteria:

- App Creator, Chat, App Store, CLI, and external/inbound route are covered by
  tests or evidence.

Phase update checklist:

- [ ] App Creator route covered.
- [ ] Business OS Chat route covered.
- [ ] App Store or template route covered.
- [ ] CTOX CLI route covered.
- [ ] External MCP or inbound communication route covered.
- [ ] All covered routes attach the same skill/resource metadata.

### Phase 7: Production Signoff

Status: `pending`

Tasks:

- Write final evidence summary.
- Document known limitations.
- Ensure release install works via `ctox upgrade --dev`.
- Push main.

Exit criteria:

- CTOX can create and modify small Business OS apps on the release install path
  without source-checkout assumptions.

Phase update checklist:

- [ ] `ctox upgrade --dev` release path tested.
- [ ] Fresh installed runtime path tested.
- [ ] No generated app artifact under `src/`.
- [ ] Five-app bench green in CTOX with MiniMax M3.
- [ ] Modification flow tested for at least one generated app.
- [ ] Main pushed.

## Evidence Entry Template

Copy this template into the Evidence Log for every material run or patch.

```md
### YYYY-MM-DD <short title>

- Phase:
- Owner:
- Run id / task ids:
- Commands:
- Changed files:
- Evidence path:
- Result:
- Failure classification:
- Follow-up:
```

## Evidence Log

Append one entry per meaningful run.

### 2026-06-20 Baseline Cleanup

- Commits: `e8bec3b8`, `b142e4c8`
- Installed release: `branch-main-20260620T102259Z`
- Result: App Creator direct-file writing removed; skill converted to resource
  index; installed current tree scanned clean for old Creator/skill builder
  artifacts.
- Remaining blocker: CTOX-native five-app bench has not yet passed end to end
  after the cleanup.

### 2026-06-20 CTOX-Native Bench Runner

- Phase: 2 and Phase 3 start
- Owner: Codex
- Run id / task ids: `rcli`;
  `queue:system::39a76fa1e7a3615e37395591`,
  `queue:system::83f0021294eb8cb4a41c34a9`,
  `queue:system::81a6b65f041a523efc1134a6`,
  `queue:system::b669a5ad3773b56abbd2d5c9`,
  `queue:system::7de58c08014601d6dcf2adfb`
- Commands:
  `cargo test --bin ctox app_bench_`;
  `cargo run --bin ctox -- business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rcli --actor rxdb-command`;
  `ctox upgrade --dev`;
  `ctox business-os app bench --help`
- Changed files:
  `src/core/service/business_os.rs`,
  `src/core/main.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `runtime/business-os/app-creation-bench/rcli/events.jsonl`,
  `runtime/business-os/app-creation-bench/rcli/summary.json`
- Result: runner submitted five real `ctox.business_os.app.create` commands
  through `accept_rxdb_business_command`; tests prove it writes no app
  artifacts and rejects retired `128k` context. Dev upgrade installed
  `branch-main-20260620T113510Z`, and the installed CLI exposes the bench
  command.
- Failure classification: none for Phase 2; Phase 3 remains incomplete because
  worker execution, validation, browser smoke, and automation smoke have not
  been observed.
- Follow-up: let CTOX workers process the five queued tasks, then collect
  validation/browser/automation evidence and classify failures before patching.

### 2026-06-20 Editable Plan Ledger

- Phase: 3
- Owner: Codex
- Run id / task ids: `rcli`
- Commands: `sed -n '1,260p' docs/business-os-app-creation-plan.md`,
  `git diff -- docs/business-os-app-creation-plan.md`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path: this file
- Result: added a Current Execution Slice with concrete update checkboxes and
  explicit editable/stable sections so continuation agents can track progress
  in-place during execution.
- Failure classification: none; planning/evidence hygiene update.
- Follow-up: keep this section current while collecting bench status,
  validation, browser smoke, automation smoke, and failure classifications.

### 2026-06-20 Bench Status Snapshot

- Phase: 3
- Owner: Codex
- Run id / task ids: `rcli`;
  `queue:system::39a76fa1e7a3615e37395591`,
  `queue:system::83f0021294eb8cb4a41c34a9`,
  `queue:system::81a6b65f041a523efc1134a6`,
  `queue:system::b669a5ad3773b56abbd2d5c9`,
  `queue:system::7de58c08014601d6dcf2adfb`
- Commands:
  `cargo test --bin ctox app_bench_`;
  `cargo run --bin ctox -- business-os app bench status --run-id rcli --validate`;
  `ctox status`;
  `jq '{bench_green,needs_attention,counts,status_path, apps: [.apps[] | {case,module_id, route_status:.queue.route_status, validation_ran:.validation.ran, validation_ok:.validation.ok, artifacts_exist:.artifacts.exists, tests_present:.artifacts.tests_present, missing:.artifacts.required_missing}]}' runtime/business-os/app-creation-bench/rcli/status-1781956817565.json`
- Changed files:
  `src/core/service/business_os.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `runtime/business-os/app-creation-bench/rcli/status-1781956817565.json`,
  `runtime/business-os/app-creation-bench/rcli/events.jsonl`
- Result:
  `bench_green=false`, `needs_attention=true`; counts are `handled=2`,
  `leased=1`, `pending=2`, `validation_passed=2`, `validation_skipped=3`,
  `artifact_dirs_present=2`, `artifact_dirs_missing=3`,
  `apps_with_missing_required_files=3`. `bench_subscriptions_rcli` and
  `bench_quality_rcli` exist under `runtime/business-os/installed-modules/`
  and pass `ctox business-os app validate <module-id> --installed`.
  `bench_projects_rcli` is leased by `ctox-service` and currently has no module
  directory. `bench_inventory_rcli` and `bench_contracts_rcli` remain pending
  with no module directory. A first `ctox status` check reported the CTOX
  service running but idle; a follow-up check reported `busy=true`,
  `worker_active_count=1`, and `pending_count=3`.
- Failure classification:
  no app artifact or skill-resource failure is proven for the three unfinished
  apps yet. Earlier service idleness is a `runtime_orchestration_gap` candidate,
  but the follow-up status shows active work; continue observing before
  patching orchestration.
- Follow-up:
  let the leased task finish or fail, then collect another status snapshot. Do
  not patch app outputs or skill resources until unfinished tasks are terminal
  or their failure class is clear. After the remaining tasks complete, collect
  validation, browser smoke, and automation smoke evidence.

### 2026-06-20 Installed Bench Status After Dev Upgrade

- Phase: 3
- Owner: Codex
- Run id / task ids: `rcli`;
  `queue:system::39a76fa1e7a3615e37395591`,
  `queue:system::83f0021294eb8cb4a41c34a9`,
  `queue:system::81a6b65f041a523efc1134a6`,
  `queue:system::b669a5ad3773b56abbd2d5c9`,
  `queue:system::7de58c08014601d6dcf2adfb`
- Commands:
  `ctox upgrade --dev`;
  `ctox business-os app bench --help`;
  `ctox business-os --help | rg "app bench (run|status)"`;
  `ctox business-os app bench status --run-id rcli --validate`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rcli/status-1781957954819.json`
- Result:
  dev upgrade initially failed because the local disk was full during state
  backup. Only build cache under
  `/Users/michaelwelsch/.cache/ctox/cargo-target/ctox-main` was removed; state
  backups and runtime state were not deleted. The retry installed
  `branch-main-20260620T120455Z`, and the installed CLI exposes both
  `ctox business-os app bench run` and `ctox business-os app bench status`.
  The installed status snapshot reports `bench_green=false`,
  `needs_attention=true`, `handled=2`, `leased=1`, `pending=1`,
  `other=1`, `validation_passed=2`, `validation_failed=1`,
  `validation_skipped=2`, `artifact_dirs_present=3`, and
  `artifact_dirs_missing=2`. `bench_subscriptions_rcli` and
  `bench_quality_rcli` pass installed validation. `bench_projects_rcli` has
  complete installed artifacts but fails its own tests. `bench_inventory_rcli`
  is leased by `ctox-service`. `bench_contracts_rcli` is pending.
- Failure classification:
  `bench_projects_rcli` is currently `model_failure`: the agent produced a
  static-valid installed app with command-bus automation shape, but its own
  generated helper tests contradict the generated implementation. Evidence:
  `toMs` intentionally parses `YYYY-MM-DD` to UTC midnight while the test
  expects a timestamped ISO string to preserve `10:00:00Z`; another test
  asserts a unified follow-up list has length `3` and also expects four titles.
  This is not enough evidence for a new skill rule, validator change, or
  deterministic builder. The unfinished inventory and contracts tasks have no
  failure class yet.
- Follow-up:
  let CTOX review/rework the project app or mark it terminal, let inventory and
  contracts finish, then collect another installed `bench status --validate`
  snapshot. Patch only if a repeated architecture-level class appears.

### 2026-06-20 App Lifecycle Visibility Gap

- Phase: 3 classification and Phase 4 systemic fix
- Owner: Codex
- Run id / task ids: `rcli`; validation-green app
  `bench_subscriptions_rcli`
- Commands:
  `ctox business-os app bench status --run-id rcli --validate`;
  browser smoke against `http://127.0.0.1:8765/#bench_subscriptions_rcli`;
  SQLite inspection of installed `business_module_catalog`,
  `business_module_versions`, `business_module_acl`, and `business_users`;
  `cargo test --bin ctox app_bench_`;
  `cargo test --bin ctox app_validation_success_accepts_postlease_artifact_write`
- Changed files:
  `src/core/business_os/store.rs`,
  `src/core/service/business_os.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rcli/status-1781958189008.json`
- Result:
  browser smoke could not open the validation-green Subscriptions app. The
  shell stayed on Desktop even with hash `#bench_subscriptions_rcli`. Installed
  catalog inspection showed the bench apps existed, but their lifecycle fields
  had empty `creator_user_id`, empty `responsible_user_ids`, and no founder ACL
  rows. The local installed Business OS user was `local-dev`, while the old
  bench run had used `rxdb-command`. The source patch now defaults fresh bench
  tasks to the local Business OS user and makes app-create validation success
  seed the creating actor as founder/responsible and record an initial
  `app_create` module version before catalog projection.
- Failure classification:
  `runtime_orchestration_gap`. This was not a skill prompt issue, not an app
  output issue, and not evidence for a deterministic builder.
- Follow-up:
  run `git diff --check`, commit and push the patch, install it via
  `ctox upgrade --dev`, then start a fresh five-app bench without `--actor`.
  Browser smoke must be repeated on the fresh validation-green apps.

### 2026-06-20 Fresh Bench Reference Gap

- Phase: 4 systemic fix
- Owner: Codex
- Run id / task ids: `rfix1`;
  `queue:system::9033ab1f0861ebf354c6d054`,
  `queue:system::c67e02fcd32f49606690ea7a`,
  `queue:system::18367dff92b11256a37261ff`,
  `queue:system::05e242d95bb62b270ea40562`,
  `queue:system::1204449be9c9d74ad281e6c7`
- Commands:
  `ctox upgrade --dev`;
  `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix1`;
  `ctox business-os app bench status --run-id rfix1 --validate`;
  SQLite inspection of `business_commands.client_context_json`;
  `ctox business-os app references --json`;
  `cargo test --bin ctox app_references_mark_source_only_manifest_fields_as_non_templates`;
  `cargo test --bin ctox app_bench_`
- Changed files:
  `src/core/service/business_os.rs`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix1/status-1781960796182.json`
- Result:
  release `branch-main-20260620T124515Z` installed successfully and `rfix1`
  submitted five real app-create commands. All five command contexts use actor
  `local-dev`, proving the lifecycle default actor path is active. The
  Inventory task produced partial artifacts under
  `runtime/business-os/installed-modules/bench_inventory_rfix1`, but validation
  remained red. The generated `module.json` copied source-only patterns:
  `store.installable: true`, `layout.icon_svg`, inline SVG markup, and
  `layout.right` without `layout.third_pane_justification`. It also had helper
  test failures. The installed reference catalog exposed those source-manifest
  patterns directly in `layout`, and internal developer tools appeared as
  normal reference candidates.
- Failure classification:
  `reference_gap` for the source-manifest copying signal. Helper-test failures
  in the same partial app remain `model_failure` until repeated after the
  reference fix. The still-pending apps are not classified yet.
- Follow-up:
  commit and push the reference-catalog/skill-resource patch, install it
  through `ctox upgrade --dev`, then start a fresh bench run. Do not repair
  `bench_inventory_rfix1` directly.

### 2026-06-20 Reference Fix Installed

- Phase: 4 closeout and Phase 5 start
- Owner: Codex
- Run id / task ids: no fresh post-fix run yet; pre-fix run `rfix1` still has
  queued or active tasks in the service.
- Commands:
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox status`;
  `ctox business-os app references --json`
- Changed files:
  `src/core/service/business_os.rs`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/module-contract.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/dos-and-donts.md`,
  `src/skills/system/product_engineering/business-os-app-module-development/references/architecture-translation.md`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  installed release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T130820Z`;
  installed runtime store
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os.sqlite3`
- Result:
  `ctox upgrade --dev` installed `branch-main-20260620T130820Z` from main.
  The installed reference API now returns `runtime_rules`, marks normal
  business-workflow references as
  `recommended_for_generated_business_app: true`, and emits source-manifest
  warnings for fields such as `layout.icon_svg`, `store.installable`, and
  unqualified `layout.right`. `ctox status` reports the service running with
  WebRTC/RxDB replication up, but it is still busy with pre-reference-fix
  bench tasks. Those tasks are useful only as forensic evidence; they are not
  production-readiness proof for the installed reference fix.
- Failure classification:
  no new failure classification from this install step. Phase 5 must start with
  a fresh bench or explicitly record why old queued pre-fix tasks were
  superseded.
- Follow-up:
  wait for or supersede pre-fix `rfix1` tasks, run a fresh five-app bench after
  release `branch-main-20260620T130820Z`, collect validation/browser/
  persistence/automation evidence, and classify failures before editing code or
  skill resources again.

### 2026-06-20 Supersede Pre-Fix Bench Tasks

- Phase: 5
- Owner: Codex
- Run id / task ids:
  superseded run `rfix1`:
  `queue:system::9033ab1f0861ebf354c6d054`,
  `queue:system::c67e02fcd32f49606690ea7a`,
  `queue:system::18367dff92b11256a37261ff`,
  `queue:system::05e242d95bb62b270ea40562`,
  `queue:system::1204449be9c9d74ad281e6c7`;
  accidental run `r1781961729513`:
  `queue:system::64c5db8a79db1abdc743b1fe`
- Commands:
  `ctox queue cleanup-scope --match-run-id rfix1 --status pending --status leased --dry-run --cancel-open`;
  `ctox queue cleanup-scope --match-run-id rfix1 --status pending --status leased --cancel-open`;
  `ctox queue cleanup-scope --match-run-id r1781961729513 --status pending --status leased --dry-run`;
  `ctox queue cleanup-scope --match-run-id r1781961729513 --status pending --status leased --cancel-open`;
  `ctox stop`;
  `ctox start`;
  `ctox status`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix1/status-1781961712898.json`;
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/r1781961729513/events.jsonl`
- Result:
  `rfix1` still had five open tasks after the reference fix was installed: four
  pending and one leased. Because those tasks were created before the
  reference-catalog fix, they were cancelled as superseded through the official
  queue cleanup command. A CLI usability bug was also exposed: invoking
  `ctox business-os app bench run --help` did not show help and instead started
  an unintended default run `r1781961729513` far enough to submit one
  Subscriptions task. That task was cancelled through the same cleanup path.
  The service stayed busy in the already-running cancelled slice, so it was
  restarted. After restart, `ctox status` reported `busy=false`,
  `pending_count=0`, and Business OS web/MCP autostarted.
- Failure classification:
  `runtime_orchestration_gap` for the stale running cancelled slice;
  `validator_gap` is not implicated. `bench run --help` is a
  `bench_cli_guard_gap`, tracked as an Open Issue because the bench CLI must
  never submit work for help/usage requests.
- Follow-up:
  commit and later install the bench CLI help/usage guard after the active
  `rfix2` run is no longer running. Do not run `ctox upgrade --dev` while the
  fresh bench is active.

### 2026-06-20 Post-Reference Bench Started

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::f4efcb9ad60fb6ab0c35a495`,
  `queue:system::33aefcd7d41e5428b182d0a1`,
  `queue:system::e48125df6171032164adb91c`,
  `queue:system::ed23aa850a4334f4ab4f3303`,
  `queue:system::105d99183374108030b4ea9c`
- Commands:
  `ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k --run-id rfix2`;
  SQLite check of `business_commands.client_context_json`;
  `ctox queue list --status pending --status leased --limit 10`;
  `ctox business-os app bench status --run-id rfix2 --validate`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/events.jsonl`;
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781962112290.json`
- Result:
  fresh post-reference-fix run `rfix2` submitted five real app-create commands
  and accepted all five. All command contexts use actor `local-dev` and
  context `256k`. The latest snapshot shows `bench_green=false`,
  `needs_attention=true`, `pending=4`, `leased=1`, `handled=0`,
  `artifact_dirs_present=0`, and `validation_skipped=5`; Quality is leased by
  `ctox-service` and still running with no app artifacts yet.
- Failure classification:
  none yet for app output. The run is in progress, so missing artifact
  directories are expected until the leased worker writes files or terminates.
- Follow-up:
  keep monitoring `rfix2`. Do not patch skill resources, validation, or
  generated app artifacts until there is terminal evidence or repeated failure
  evidence from the post-reference-fix run.

### 2026-06-20 Bench CLI Help Guard Patched

- Phase: 5
- Owner: Codex
- Run id / task ids: not an app run; follows accidental run `r1781961729513`
- Commands:
  `cargo test --bin ctox app_bench_help_does_not_submit_or_cleanup`;
  `cargo test --bin ctox app_bench_`;
  `git diff --check -- src/core/service/business_os.rs docs/business-os-app-creation-plan.md`
- Changed files:
  `src/core/service/business_os.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path: source test output in this work block
- Result:
  source patch makes `ctox business-os app bench run --help` and
  `ctox business-os app bench status --help` return usage before cleanup,
  evidence writes, or Business OS command submission. Regression test
  `app_bench_help_does_not_submit_or_cleanup` passed, and the full
  `app_bench_` test filter passed with 4 tests.
- Failure classification:
  `bench_cli_guard_gap`, limited to the bench evidence tool. This did not
  change app-generation behavior, skill resources, validators, or generated app
  artifacts.
- Follow-up:
  commit the source patch and plan update. Install it later with
  `ctox upgrade --dev` only when doing so will not interrupt the active
  post-reference bench evidence run.

### 2026-06-20 rfix2 Partial Status Snapshot

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::f4efcb9ad60fb6ab0c35a495`,
  `queue:system::33aefcd7d41e5428b182d0a1`,
  `queue:system::e48125df6171032164adb91c`,
  `queue:system::ed23aa850a4334f4ab4f3303`,
  `queue:system::105d99183374108030b4ea9c`
- Commands:
  `ctox business-os app bench status --run-id rfix2 --validate`;
  `ctox status`;
  `ctox queue list --status pending --status leased --status review_rework --limit 20`
- Changed files: `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781964567806.json`
- Result:
  `bench_green=false`, `needs_attention=true`; counts are `pending=3`,
  `leased=1`, `other=1`, `validation_passed=1`, `validation_skipped=4`,
  `artifact_dirs_present=1`, and `artifact_dirs_missing=4`.
  `bench_inventory_rfix2` has complete installed artifacts, 12 files, and
  passes installed validation plus 17 helper tests. `bench_quality_rfix2` is in
  `review_rework` with no artifact directory after worker-error validation.
  Subscriptions, Projects, and Contracts are still pending with no artifact
  directories. `ctox status` reports `busy=false`, `worker_active_count=0`,
  and `pending_count=4`, with pending previews for Projects, Contracts,
  Subscriptions, and Inventory.
- Failure classification:
  Inventory is not a failure yet because the app artifacts validate, but the
  task has not finalized. Quality is a `runtime_orchestration_gap` candidate
  because no app output exists and the task reached rework after worker error.
  The idle pending queue state is also a `runtime_orchestration_gap` candidate,
  but do not patch until the next status check confirms it is stuck rather than
  normal cadence.
- Follow-up:
  keep Phase 5 active. Do not edit generated app artifacts. If the queue stays
  idle with pending/leased work, investigate worker scheduling/recovery before
  adding skill rules. Browser and automation smoke should wait until a task is
  terminal and validation-green.

### 2026-06-20 Runtime Module ID Finalization Gap

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::33aefcd7d41e5428b182d0a1`
- Commands:
  `ctox business-os app validate bench_inventory_rfix2 --installed`;
  `cargo test --bin ctox app_validation_success_preserves_runtime_module_id_with_underscores`;
  `cargo test --bin ctox app_validation_success_`;
  `cargo test --bin ctox business_os_app_validation_worker_error_after_green_completes_business_command`;
  `cargo test --bin ctox app_bench_`;
  `git diff --check -- src/core/business_os/store.rs docs/business-os-app-creation-plan.md`
- Changed files:
  `src/core/business_os/store.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  direct installed CLI output from release `branch-main-20260620T130820Z`;
  source regression test output in this work block
- Result:
  direct installed validation of `bench_inventory_rfix2` printed
  `Business OS app artifact validation OK: bench_inventory_rfix2 (installed mode)`
  and then failed finalization with
  `Error: module bench-inventory-rfix2 was not found`. The source patch changes
  app-version snapshot recording to preserve the validated runtime-installed
  module id instead of reusing the source-module slug sanitizer. The regression
  test proves that `bench_inventory_rfix2` completes as `handled`, records a
  `business_module_versions` row under `bench_inventory_rfix2`, records none
  under `bench-inventory-rfix2`, and remains present in the RxDB module
  catalog.
- Failure classification:
  `runtime_orchestration_gap`. This was a CTOX lifecycle/versioning bug for
  runtime-installed app ids with underscores, not a model failure, skill issue,
  validator gap, or generated-app issue.
- Follow-up:
  commit and push the source fix, install it through `ctox upgrade --dev`, then
  rerun direct validation/finalization for `bench_inventory_rfix2`. If
  Inventory becomes handled, continue Phase 5 by checking whether the remaining
  pending/rework tasks dispatch correctly or need a fresh post-fix bench.

### 2026-06-20 Runtime Module ID Fix Installed

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::33aefcd7d41e5428b182d0a1`,
  `queue:system::e48125df6171032164adb91c`,
  `queue:system::105d99183374108030b4ea9c`,
  `queue:system::f4efcb9ad60fb6ab0c35a495`,
  `queue:system::ed23aa850a4334f4ab4f3303`
- Commands:
  `ctox upgrade --dev`;
  `readlink /Users/michaelwelsch/.local/lib/ctox/current`;
  `ctox business-os app validate bench_inventory_rfix2 --installed`;
  `ctox business-os app bench run --help`;
  `ctox business-os app bench status --run-id rfix2 --validate`;
  `ctox queue show --message-key queue:system::33aefcd7d41e5428b182d0a1`;
  `ctox status`
- Changed files:
  `src/core/business_os/store.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  installed release
  `/Users/michaelwelsch/.local/lib/ctox/releases/branch-main-20260620T141728Z`;
  status snapshot
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781965773118.json`
- Result:
  release `branch-main-20260620T141728Z` is the active installed release.
  Direct installed validation of `bench_inventory_rfix2` now returns
  `Business OS app artifact validation OK: bench_inventory_rfix2 (installed mode)`
  without the old `bench-inventory-rfix2` finalization error. The Inventory
  queue task is `handled` with status note
  `business-os:terminal-success: app validation passed`. The installed
  `bench run --help` path returns usage and reports
  `submits_real_business_commands: false`, so the help guard is installed.
  Current `rfix2` status is still not green: Inventory is handled and
  validation-green; Projects is leased with partial schema-only artifacts and
  validation red while still running; Quality is in `review_rework` with no
  artifact directory; Subscriptions and Contracts are pending. `ctox status`
  reports the service running and busy with one queue worker on Projects.
- Failure classification:
  the Inventory failure is resolved as `runtime_orchestration_gap`. The
  remaining `rfix2` failures are not yet final: Projects is active, two apps
  are pending, and Quality is the only current no-artifact rework case.
- Follow-up:
  keep Phase 5 running. Do not patch generated app files or skill resources
  while Projects is active. After the active worker reaches terminal evidence,
  collect a fresh bench status snapshot, then classify Quality, pending apps,
  browser smoke, persistence smoke, and automation smoke before any further
  source change.

### 2026-06-20 rfix2 Projects Green And Rework Marker Gap

- Phase: 5
- Owner: Codex
- Run id / task ids: `rfix2`;
  `queue:system::33aefcd7d41e5428b182d0a1`,
  `queue:system::e48125df6171032164adb91c`,
  `queue:system::105d99183374108030b4ea9c`,
  `queue:system::f4efcb9ad60fb6ab0c35a495`,
  `queue:system::ed23aa850a4334f4ab4f3303`
- Commands:
  `ctox business-os app bench status --run-id rfix2 --validate`;
  `ctox status`;
  `ctox queue list --status pending --status leased --status review_rework --limit 20`;
  `cargo test --bin ctox app_validation_success_`;
  `cargo test --bin ctox worker_idle_cleanup_leases_next_durable_queue_after_busy_clears`;
  `cargo test --bin ctox business_os_app_validation_rework_is_leased_before_fresh_pending_app_tasks`;
  `git diff --check -- src/core/service/service.rs`
- Changed files:
  `src/core/service/service.rs`,
  `docs/business-os-app-creation-plan.md`
- Evidence path:
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781966787521.json`
- Result:
  `rfix2` now has two terminal-green apps. Inventory and Projects are
  `handled`, installed validation passes, and helper tests pass
  (`bench_inventory_rfix2`: 17 tests; `bench_projects_rfix2`: 27 tests).
  Quality remains `review_rework` with no artifact directory. Subscriptions
  and Contracts remain pending. `ctox status` reports the service running but
  idle with `pending_count=2`, `worker_active_count=0`, and work-hours not
  blocking. Source forensics found that current validation feedback prompts
  start with `Business OS app validation failed.`, while the dispatcher
  recognized only the older `Business OS app artifact validation failed.`
  marker. The targeted regression test initially leased a fresh pending app
  before rework; after the source patch it leases validation rework before
  fresh pending app tasks.
- Failure classification:
  `runtime_orchestration_gap`. The current failure is not caused by generated
  app code, skill resources, or deterministic builder absence. The installed
  dispatcher fails to see existing durable validation-rework work because the
  feedback header changed.
- Follow-up:
  commit and push the source patch plus this plan update, install through
  `ctox upgrade --dev`, then verify the installed dispatcher no longer idles.
  Before resuming active proof, cancel only superseded old-run `rcli`
  validation-rework tasks so `rfix2` Quality rework is not starved by stale
  forensic work. Do not patch generated app files.

## Handoff Notes

Latest handoff:

- Continue Phase 5.
- Use run id `rcli` as forensic evidence, not as the green proof. It was run
  through the old actor/lifecycle path.
- Use run id `rfix1` as evidence that the lifecycle actor fix works and that
  the installed reference catalog still taught source-only manifest patterns.
  It was started before the reference-catalog patch was installed.
- Latest installed queue/artifact/validator status has been captured in
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix1/status-1781960796182.json`.
- The reference-catalog patch is installed in release
  `branch-main-20260620T130820Z`.
- The runtime module-id finalization fix and bench help guard are installed in
  release `branch-main-20260620T141728Z`.
- The next useful proof is the current `rfix2` bench or a later fresh bench
  after release `branch-main-20260620T141728Z`; old queued `rfix1` tasks were
  created before the reference release and cannot prove production readiness.
- Active run `rfix2` is the current proof attempt. Latest status is
  `/Users/michaelwelsch/.local/lib/ctox/current/runtime/business-os/app-creation-bench/rfix2/status-1781966787521.json`.
- In `rfix2`, Inventory and Projects are terminal-green and `handled`; direct
  installed validation passes for both. Quality has no artifacts and is in
  `review_rework`; Subscriptions and Contracts are still pending.
- `ctox status` currently reports the service running but idle with
  `pending_count=2`, `worker_active_count=0`, work-hours disabled, and no
  active source label. This is explained by the installed dispatcher not
  recognizing the current validation-rework prompt header.
- Source patch in `src/core/service/service.rs` accepts both
  `Business OS app validation failed.` and
  `Business OS app artifact validation failed.` as validation-rework markers.
  The targeted regression test is green. The patch still needs commit, push,
  `ctox upgrade --dev`, and installed-path verification.
- Before installed dispatcher verification, cancel only superseded old-run
  `rcli` validation-rework tasks if they would otherwise take priority over
  `rfix2`. Do not cancel current `rfix2` tasks.
- Do not patch generated app files.
- Do not patch skill resources, validators, or orchestration for the old
  project helper-test failure unless the same failure class repeats or exposes
  a real architecture gap.
- Update Current Execution Slice checkboxes before ending the next work block.

## Open Issues

- Define validator behavior for internal shell tools such as App Creator.
- Complete the first five-app bench run through CTOX workers with MiniMax M3.
- Rerun or continue after installing the rework-marker patch; old `rfix1`
  evidence is not enough for production readiness because it used the old
  reference output, and current `rfix2` is only two-fifths green.
- Track `bench_projects_rcli` as a current model-output failure unless CTOX
  review/rework repairs it or the same inconsistency repeats across more runs.
- Track `bench_inventory_rfix1` helper-test failures as model-output failures
  unless they repeat after the reference-catalog patch is installed.
- Add or wire wait/status collection for bench task completion beyond the
  current read-only status snapshot.
- Add browser smoke and automation smoke collection to bench evidence.
- Confirm the installed reference ranking prevents normal app creation from
  overfitting to internal developer tools.
- Confirm every app creation entry point attaches the same structured skill
  resource context.
- Install and verify the rework-marker patch with `ctox upgrade --dev`; the
  installed dispatcher must lease validation rework or pending app tasks
  instead of staying idle.
- Investigate `bench_quality_rfix2` as a no-artifact validation-rework case
  after the installed dispatcher can actually pick up current rework prompts.
  Do this as orchestration/model forensics, not by changing generated app
  files.
