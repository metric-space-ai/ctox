---
name: incident-response
description: Triage active incidents from alerts or failures into explicit hypotheses, mitigations, evidence, and next actions. Use when CTOX must react to outages, latency spikes, bad deploys, disk pressure, failing jobs, or repeated alert clusters and keep a crisp incident timeline grounded in concrete system state.
---

# Incident Response

Use this skill when there is a live user-visible or operator-visible failure that needs explicit stabilization work.

Do not use it for broad scope discovery or routine health review:

- use `discovery_graph` when scope is unclear
- use `reliability_ops` for health analysis without urgent containment
- use `change_lifecycle` when the task is a planned rollback or controlled change

This skill uses the shared SQLite kernel via `skill_key=incident_response`.

## Operating Model

Preferred helper scripts under `scripts/`:

- `incident_collect.py`
- `incident_capture_run.py`
- `incident_store.py`
- `incident_query.py`
- `incident_bootstrap.py`

They are open helper resources. Read them, patch them, or bypass them when the incident shape requires it.

## Tool Contracts

- `incident.capture_raw`
- `incident.store_capture`
- `incident.store_graph`
- `incident.query`
- `incident.bootstrap_case`

## Workflow

1. State the symptom, affected scope, and urgency.
2. Capture first evidence before acting.
3. Build a small hypothesis set.
4. Prefer the smallest mitigation that matches the evidence.
5. Persist an `incident_case`, `hypothesis_set`, `mitigation_action`, and `status_update`.
6. If a real mutation is needed, hand the next slice to `change_lifecycle`.

## Operator Feedback Contract

Answer for the operator first, not for SQLite.

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

If you stabilized part of the incident but did not finish the job, do not imply silent continuation. Say the incident is still open and point to the durable next-work record.

## Completion Gate

Do not finish the reply until all of the following are true:

- all seven headings are present
- `State` is explicit
- mitigation and evidence are clearly separated
- if the incident remains open, a durable next slice exists in queue or plan state
- if no durable next slice exists yet, the reply stays `blocked`

## Guardrails

- Do not claim root cause from one symptom.
- Keep mitigations narrow.
- Record evidence and action sequence.
- If the incident is not resolved, leave an explicit next slice.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/incident-commands.md](references/incident-commands.md)
- [references/triage-rules.md](references/triage-rules.md)
