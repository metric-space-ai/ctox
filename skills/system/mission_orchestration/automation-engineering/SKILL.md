---
name: automation-engineering
description: Replace repeated manual operational work with safe scripts, queued tasks, or scheduled CTOX work. Use when the same host or admin procedure keeps recurring, an incident pattern should become automation, or a manual checklist should be turned into tested repo-managed automation without bypassing the existing CTOX queue and schedule flow.
cluster: mission_orchestration
---

# Automation Engineering

Automation design and operating assumptions count as durable knowledge only when they are reflected in SQLite-backed runtime state. Workspace plans or notes do not count as durable knowledge by themselves.

Use this skill when repeated operational work should become a repo script, queued slice, or scheduled CTOX task.

Do not use it for live incident handling or one-off system changes:

- use `incident_response` for active stabilization
- use `change_lifecycle` for deliberate state changes
- use `ops_insight` for reporting that does not yet justify automation

This skill uses the shared SQLite kernel via `skill_key=automation_engineering`.

## Operating Model

Preferred helper scripts under `scripts/`:

- `automation_collect.py`
- `automation_capture_run.py`
- `automation_store.py`
- `automation_query.py`
- `automation_bootstrap.py`

These are open helper resources.

## Tool Contracts

- `automation.capture_raw`
- `automation.store_capture`
- `automation.store_graph`
- `automation.query`
- `automation.bootstrap_recipe`

## Workflow

1. Confirm the pattern is really repeated.
2. Capture current CTOX queue/schedule/plan state and repo automation hints.
3. Persist a `task_pattern`, `automation_recipe`, `workflow_version`, `test_evidence`, and `adoption_note`.
4. Keep the interface explicit and dry-run first.
5. Route real rollout work into `change_lifecycle`.

## Operator Feedback Contract

Answer for the operator first.

Use these exact headings:

- `**Status**`
- `**State**`
- `**Scope**`
- `**Autonomous Actions**`
- `**Escalation**`
- `**Current Findings**`
- `**Next Step**`

`State` must be one of:

- `proposed`
- `prepared`
- `executed`
- `blocked`

Automation design, recipes, schedules, and test artifacts are `prepared`. Only call it `executed` when the recurring or scripted path was actually activated or changed.

## Completion Gate

Do not finish the reply until all of the following are true:

- all seven headings are present
- proposed automation and active automation are clearly separated
- test evidence is mentioned before claiming the automation is ready
- if rollout is deferred, a durable next slice exists instead of an implied continuation

## Guardrails

- No hidden loops.
- Keep recurring work in CTOX schedule and queue.
- Do not call something automated until it is testable and rerunnable.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/automation-patterns.md](references/automation-patterns.md)
- [references/automation-rules.md](references/automation-rules.md)
