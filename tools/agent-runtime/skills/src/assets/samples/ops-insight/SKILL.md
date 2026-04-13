---
name: ops-insight
description: Condense queue state, plans, schedules, incidents, resource pressure, and change results into operational scorecards and decision briefs. Use when CTOX needs to summarize ongoing operations, highlight risk, rank backlog, or produce evidence-based daily or weekly ops reviews from the existing CTOX substrate and concrete host facts.
---

# Ops Insight

Use this skill when the job is to turn existing CTOX and host state into a compact decision surface or report.

Do not use it to gather domain evidence that belongs to another skill:

- use `discovery_graph` for scope and inventory
- use `reliability_ops` for health
- use `incident_response` for live incidents
- use `security_posture` or `recovery_assurance` for those specific domains

This skill uses the shared SQLite kernel via `skill_key=ops_insight`.

## Operating Model

Preferred helper scripts under `scripts/`:

- `ops_collect.py`
- `ops_capture_run.py`
- `ops_store.py`
- `ops_query.py`
- `ops_bootstrap.py`

These are open helper resources.

## Tool Contracts

- `ops.capture_raw`
- `ops.store_capture`
- `ops.store_graph`
- `ops.query`
- `ops.bootstrap_report`

## Workflow

1. Pull only the needed substrate.
2. Capture current CTOX state, a host brief, and optional shared-kernel summaries.
3. Persist a `scorecard`, `decision_brief`, `priority_backlog`, and `dashboard_view`.
4. Keep facts, risks, and next actions separate.

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

Reports, scorecards, and briefings are normally `prepared`. Only use `executed` if CTOX actually changed an operational control, backlog, schedule, or other live state.

## Completion Gate

Do not finish the reply until all of the following are true:

- all seven headings are present
- scope and timeframe are explicit
- facts, risks, and recommendations are clearly separated
- if the report implies further work, the next slice points to the correct execution skill or a durable queue/plan record

## Guardrails

- No invented KPIs.
- Always state scope and timeframe.
- Point action items back to the right execution skill.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/ops-reporting.md](references/ops-reporting.md)
- [references/insight-rules.md](references/insight-rules.md)
