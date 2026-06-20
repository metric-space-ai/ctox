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

Do not use this plan as an app-building prompt. CTOX must still build apps
through normal agent execution, the Business OS app-module skill resources, and
the Business OS command/task pipeline.

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
- `ctox upgrade --dev` installed the current main release after these changes.
- App creation is not yet production-ready until CTOX-native bench runs pass
  end to end.

## Tracker

| Phase | Status | Owner | Evidence | Notes |
| --- | --- | --- | --- | --- |
| 0. Remove wrong architecture | done | Codex | `e8bec3b8`, `b142e4c8`, installed release `branch-main-20260620T102259Z` | App Creator no longer writes app files itself; resource-index skill installed. |
| 1. Define acceptance gates | pending |  |  | Formalize what must pass for app creation, modification, validation, browser smoke, and automation. |
| 2. Build CTOX-native bench runner | pending |  |  | Runner must submit real Business OS app-create tasks and collect evidence. |
| 3. Run five-app bench in CTOX | pending |  |  | Use MiniMax M3 through CTOX, not Codex shortcuts. |
| 4. Classify failures | pending |  |  | Separate skill/resource gaps, validator gaps, runtime orchestration gaps, and model failures. |
| 5. Patch only systemic gaps | pending |  |  | Fix repeated or architecture-level failures only. Avoid ad hoc app-specific fixes. |
| 6. Repeat until green | pending |  |  | Reset bench apps, rerun, and update this plan with each round. |
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

Status: `pending`

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

- [ ] CLI command or equivalent CTOX-native runner added.
- [ ] Runner submits real `ctox.business_os.app.create` tasks.
- [ ] Evidence path under ignored `runtime/` documented.
- [ ] Cleanup only touches bench-tagged runtime apps.
- [ ] Tests prove the runner does not write app artifacts.

### Phase 3: First Five-App CTOX Run

Status: `pending`

Tasks:

- Run all five bench apps with MiniMax M3 through CTOX.
- Record produced paths, task IDs, validation result, browser smoke result, and
  automation smoke result.
- Stop after systemic failure if continuing would only create duplicate noise.

Exit criteria:

- Every failure has an evidence-backed classification.

Phase update checklist:

- [ ] Run id recorded.
- [ ] Five queue task ids recorded.
- [ ] Produced module paths recorded.
- [ ] Validation results recorded per app.
- [ ] Browser smoke results recorded per app.
- [ ] Automation dispatch evidence recorded per app.
- [ ] Failures classified before any patch.

### Phase 4: Systemic Fixes

Status: `pending`

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

- [ ] Failure class named before patching.
- [ ] Patch scope limited to skill resource, validator, entry point, or
      orchestration gap.
- [ ] No app-specific bench repair committed.
- [ ] Regression test or evidence added.

### Phase 5: Repeat Bench

Status: `pending`

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

## Open Issues

- Define validator behavior for internal shell tools such as App Creator.
- Build the CTOX-native bench runner.
- Run fresh five-app bench through CTOX with MiniMax M3.
- Rank reference apps so normal app creation does not overfit to internal
  developer tools.
- Confirm every app creation entry point attaches the same structured skill
  resource context.
