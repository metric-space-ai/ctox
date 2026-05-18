---
name: incident-response
description: Triage active incidents from alerts or failures into explicit hypotheses, mitigations, evidence, and next actions. Use when CTOX must react to outages, latency spikes, bad deploys, disk pressure, failing jobs, or repeated alert clusters and keep a crisp incident timeline grounded in concrete system state.
cluster: host_ops
---

# Incident Response

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Only CTOX runtime store, ticket state, communication records, verification state, and direct live evidence count as durable incident knowledge. Ad hoc notes or markdown files do not count as durable knowledge by themselves.

Use this skill when there is a live user-visible or operator-visible failure that needs explicit stabilization work.

Do not use it for broad scope discovery or routine health review:

- use `discovery_graph` when scope is unclear
- use `reliability_ops` for health analysis without urgent containment
- use `change_lifecycle` when the task is a planned rollback or controlled change

This skill uses the shared CTOX knowledge store via `skill_key=incident_response`.

## Operating Model

Use CTOX CLI/API commands as the execution boundary. Do not execute embedded `scripts/` helpers from this system skill; if incident capture or persistence lacks a CTOX command, add that command before relying on it.

## Tool Contracts

- `incident.capture_raw`
- `incident.store_capture`
- `incident.store_graph`
- `incident.query`
- `incident.bootstrap_case`

## Harness Mining for Hypothesis Generation

When the incident touches harness behaviour (queue stalls, communication
failures, repeated state-machine rejections), use these CLIs to derive
hypotheses from the trigger ledger and preventive proofs rather than guessing:

```sh
ctox harness-mining causal --violation-code "<code>" --lookback 5 --limit 10
ctox harness-mining alignment --entity-type "<type>" --limit 10
```

What to read:

- `causal`: `by_violation_code[].predecessor_activity_lift[]`. The activity
  with the highest `lift` (and `support` ≥ 3) is your strongest causal
  hypothesis — record it as a `hypothesis` with the lift value as its evidence
  weight, not as a confirmed cause.
- `alignment`: `alignments[].moves[]`. Every `kind: "model"` move marks a step
  the spec demanded but the trace skipped — that is a concrete reparation
  hypothesis. Use `from_state` and `to_state` from those moves to phrase the
  mitigation, e.g. "force `<entity>` through `<missing_state>` before
  `<observed_state>`".

A `causal` lift > 5.0 with `support` ≥ 5 is strong enough to anchor the
mitigation. Below that, treat it as a working theory only.

## Workflow

1. State the symptom, affected scope, and urgency.
2. Capture first evidence before acting.
3. Build a small hypothesis set.
   For state-machine-shaped incidents, run `harness-mining causal` and
   `harness-mining alignment` first — they convert raw violation counts into
   ranked predecessor hypotheses and concrete reparation moves.
4. Prefer the smallest mitigation that matches the evidence.
   If alignment proposes a missing intermediate state, prefer mutating the
   spec or the producer — do not silently force the entity through.
5. Persist an `incident_case`, `hypothesis_set`, `mitigation_action`, and `status_update`.
   Include the top three predecessor activities from `causal` (with their
   lift) inside `hypothesis_set` evidence.
6. If a real mutation is needed, hand the next work step to `change_lifecycle`.

## Operator Feedback Contract

Answer for the operator first, not for persistence details.

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
- if the incident remains open, a durable next work step exists in queue or plan state
- if no durable next work step exists yet, the reply stays `blocked`

## Guardrails

- Do not claim root cause from one symptom.
- Keep mitigations narrow.
- Record evidence and action sequence.
- If the incident is not resolved, leave an explicit next work step.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/incident-commands.md](references/incident-commands.md)
- [references/triage-rules.md](references/triage-rules.md)
