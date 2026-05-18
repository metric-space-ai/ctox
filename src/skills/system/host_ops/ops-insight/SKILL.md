---
name: ops-insight
description: Condense queue state, plans, schedules, incidents, resource pressure, and change results into operational scorecards and decision briefs. Use when CTOX needs to summarize ongoing operations, highlight risk, rank backlog, or produce evidence-based daily or weekly ops reviews from the existing CTOX substrate and concrete host facts.
cluster: host_ops
---

# Ops Insight

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


For CTOX mission work, operational insight becomes durable only when it is reflected in the the CTOX runtime store. Free-form files or copied shell output do not count as durable knowledge by themselves.

Use this skill when the job is to turn existing CTOX and host state into a compact decision surface or report.

Do not use it to gather domain evidence that belongs to another skill:

- use `discovery_graph` for scope and inventory
- use `reliability_ops` for health
- use `incident_response` for live incidents
- use `security_posture` or `recovery_assurance` for those specific domains

This skill uses the shared CTOX knowledge store via `skill_key=ops_insight`.

## Operating Model

Use CTOX CLI/API commands as the execution boundary. Do not execute embedded `scripts/` helpers from this system skill; if an ops-insight operation lacks a CTOX command, add that command before relying on it.

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
- if the report implies further work, the next work step points to the correct execution skill or a durable queue/plan record

## Guardrails

- No invented KPIs.
- Always state scope and timeframe.
- Point action items back to the right execution skill.

## Resources

- [references/ops-reporting.md](references/ops-reporting.md)
- [references/insight-rules.md](references/insight-rules.md)
