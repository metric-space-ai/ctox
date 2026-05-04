---
name: harness-self-audit
description: Use to inspect CTOX's own harness-mining findings and decide whether the agent should pause, act, or escalate before continuing the current mission. Trigger pre-mission, post-incident, or when a confirmed harness finding has been seen in the queue.
metadata:
  short-description: Read confirmed harness findings and act on them
cluster: skill_meta
---

# Harness Self-Audit

## CTOX Runtime Contract

- Task spawning is allowed only for real execution slices that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct slice with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.

## Core Spawn Intervention Contract

When this skill is invoked because the Core Spawn Gate rejected a spawn or detected a loop, it must act as a bounded intervention and must not create new durable work.

Allowed intervention effects:

- acknowledge or mitigate an existing finding
- block the rejected child item with evidence
- consolidate evidence into the existing parent item
- mark redundant work terminal with a clear reason

Do not run commands that create new queue tasks, ticket self-work, schedules, plans, or published spills while handling a Core Spawn Gate intervention. In particular, do not use `ctox queue spill --publish`, `ctox ticket self-work-put`, `ctox schedule ensure`, or `ctox plan ingest` for the intervention path. If new work seems necessary, mark the current finding/action blocked and let the parent work item or operator decide.


Use this skill to read CTOX's own harness-mining findings and decide what
to do about them — before any high-stakes action and after any incident.

The harness-audit tick already runs in the service loop every 5 minutes. It
synthesizes a brief, applies the 2-tick confirmation gate, and persists
findings to `ctox_hm_findings`. This skill is how the agent actually
**uses** that durable signal.

## When to invoke

Invoke proactively in these three situations:

1. **Pre-mission**: before starting a multi-turn mission, an owner-visible
   communication, or a state-machine mutation in a protected lane
   (`P0FounderCommunication`, `P1RuntimeSafety`, `P1QueueRepair`). Confirmed
   harness findings change the prior on whether the mission can succeed
   safely.
2. **Post-incident**: after `incident-response` has stabilized a symptom.
   A finding that flipped to `mitigated` during the incident must be
   transitioned to `verified` once a full audit tick has passed without
   re-confirming it. A finding that is still `confirmed` after the
   incident is unfinished work, not a closed incident.
3. **On confirmed-finding event**: when a queue item or follow-up tells
   you that a harness finding was confirmed, this skill is the entry
   point for handling it. Do not bypass to ad-hoc SQL.

## Tools

```sh
ctox harness-mining brief
ctox harness-mining findings --status confirmed --limit 20
ctox harness-mining findings --status acknowledged --limit 20
ctox harness-mining findings --status mitigated --limit 20
ctox harness-mining finding-ack       --finding-id <id> [--note "<text>"]
ctox harness-mining finding-mitigate  --finding-id <id> --by <agent|operator|spec-change> [--note "<text>"]
ctox harness-mining finding-verify    --finding-id <id> [--note "<text>"]
```

For deep follow-up on a specific finding, hand off to the per-tier CLI
the brief recommends (`stuck-cases`, `causal`, `alignment`, `drift`,
`conformance`, `multiperspective`, `sojourn`, `variants`).

## Procedure

1. Run `ctox harness-mining brief`. Read three fields:
   - `status` — `healthy`, `attention_required`, or `drift_detected`.
   - `top_signal` — a one-sentence narrative of the most pressing issue.
   - `recommended_next_step` — a concrete CLI invocation. If it says
     "no action required", proceed with the original mission.

2. If `status` is not `healthy`, list current confirmed findings:
   ```sh
   ctox harness-mining findings --status confirmed --limit 20
   ```
   Each row contains `kind`, `severity`, `entity_type`, `entity_id`,
   `lane`, `evidence_json`, `detected_at`, and `confirmed_at`.

3. For every confirmed finding that touches the scope of the upcoming
   action, decide one of:

   - **Acknowledge and continue**: the finding is known and the mission
     can proceed despite it.
     `ctox harness-mining finding-ack --finding-id <id> --note "scope unrelated to <mission>"`.
   - **Mitigate before continuing**: the finding blocks safe progress.
     Run the recommended deep-CLI (e.g. `harness-mining causal` for
     hypothesis, `harness-mining alignment` for repair moves), apply the
     concrete corrective action (queue block, spec edit, retry-loop
     stop), then:
     `ctox harness-mining finding-mitigate --finding-id <id> --by agent --note "<what was done>"`.
   - **Escalate**: the mitigation requires owner approval or spec change.
     Acknowledge first, create a `ticket self-work` with the finding
     evidence, and route the mission to `blocked` rather than continuing.

4. After mitigating one or more findings, verify on the **next** audit
   tick (≥5 min later) that the finding has not re-confirmed. If the
   audit has passed cleanly:
   `ctox harness-mining finding-verify --finding-id <id> --note "post-mitigation tick clean"`.

5. Only `healthy` brief + zero confirmed findings touching the mission
   scope is a green light. `attention_required` with an acknowledged
   but un-mitigated finding is yellow — proceed only if the finding's
   scope and the mission's scope are demonstrably disjoint.

## What the agent should expect from each command

- `brief`: compact JSON with `status`, `top_signal`, `recommended_next_step`,
  `metrics` (numeric block), and `findings[]`. Always check `errors[]` —
  if a sub-algorithm failed, the brief is incomplete and should not be
  treated as a clean signal.
- `findings`: array of finding rows. The most actionable fields are
  `severity`, `kind`, `entity_id`, `evidence_json` (parse it for the
  per-kind detail), and `confirmed_at` (older = more reliable).
- Lifecycle commands (`finding-ack`, `finding-mitigate`, `finding-verify`):
  return `{ok: true, finding_id, status}` on success. They refuse to
  transition out-of-order (e.g. you cannot verify an unmitigated finding,
  you cannot mitigate a stale finding).

## Guardrails

- Do not silence a confirmed finding by acknowledging it without scope
  evidence. The acknowledgement note must state why the finding's
  entity/lane/kind is unrelated to the upcoming mission.
- Do not mitigate a finding by changing the spec without an owner-visible
  review trail. Spec changes in protected lanes
  (`P0FounderCommunication`, etc.) require `--by spec-change` and a
  ticket self-work record.
- Do not verify a finding before a clean audit tick has actually passed.
  The 2-tick confirmation gate works in both directions: just as
  detection requires two ticks, verification requires the absence of
  re-detection at the next tick.
- Do not read `ctox_hm_findings` via direct SQLite. Use the CLI; it
  enforces the lifecycle transitions that keep findings auditable.

## Operator Feedback

When this skill produces operator-facing output, use the same heading
order as `incident-response`:

1. `**Status**` — green/yellow/red of the harness, plus the brief's
   `status` value.
2. `**State**` — `proposed`, `prepared`, `executed`, or `blocked`.
3. `**Scope**` — which mission or area the audit was run for.
4. `**Autonomous Actions**` — which findings were acknowledged,
   mitigated, or verified, by the agent itself.
5. `**Escalation**` — findings that require owner approval or spec
   change.
6. `**Current Findings**` — the set of `confirmed` findings still
   touching the mission scope.
7. `**Next Step**` — either the mission continues, or the explicit
   blocker that prevents continuation.

## Do Not

- Do not run this skill in a tight loop. The audit tick already runs
  every 5 min; calling `harness-mining brief` more often than once per
  minute is wasted IO.
- Do not invoke this skill purely for visibility. If there is no
  decision to make, do not start the audit; the dashboard surface
  (TUI Settings/Harness) is the right place to *look*, not this skill.
- Do not treat `stale` findings as evidence of correctness. `stale`
  means "we did not see it again" — that can be a real recovery or a
  sampling artefact. Use `verified` only for genuine post-mitigation
  confirmations.
