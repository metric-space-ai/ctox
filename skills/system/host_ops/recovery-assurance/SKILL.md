---
name: recovery-assurance
description: Verify backup and restore readiness for hosts and services. Use when CTOX needs to inspect snapshot or backup coverage, retention, restore procedures, database dump validity, off-host copies, or disaster recovery confidence after a critical change or as a recurring assurance check.
cluster: host_ops
---

# Recovery Assurance

## CTOX Runtime Contract

- Task spawning is allowed only for real execution slices that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct slice with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Only SQLite-backed runtime state and direct recovery verification count as durable assurance evidence. Workspace notes or one-off artifacts do not count as durable knowledge by themselves.

Use this skill for backup freshness, restoreability, and disaster-recovery confidence.

Do not use it for generic filesystem health or planned change work:

- use `reliability_ops` for disk pressure or failed jobs
- use `change_lifecycle` for backup configuration changes
- use `incident_response` when the system is already in an outage

This skill uses the shared SQLite kernel via `skill_key=recovery_assurance`.

## Operating Model

Preferred helper scripts under `scripts/`:

- `recovery_collect.py`
- `recovery_capture_run.py`
- `recovery_store.py`
- `recovery_query.py`
- `recovery_bootstrap.py`

They are open helper resources.

## Tool Contracts

- `recovery.capture_raw`
- `recovery.store_capture`
- `recovery.store_graph`
- `recovery.query`
- `recovery.bootstrap_assurance`

## Harness Reparation Hypotheses

When the recoverable scope includes harness-internal flows (queue, communication,
ticket lifecycles), use alignment-based conformance to derive a concrete
restoration path before designing a runbook:

```sh
ctox harness-mining alignment --entity-type "<type>" --limit 5
```

Read `alignments[].moves[]`. Every `kind: "model"` entry tells you a state
the spec demands but the trace skipped. The minimum-cost sequence of model
moves between `from_state` and `to_state` is the smallest restoration path.
Use it to phrase the recovery procedure: "to restore `<entity>` from state
`<observed>` to `<target>`, force-traverse `<missing_states>`". Treat
`alignment_cost` as the recovery distance metric.

## Workflow

1. Name what must be recoverable.
2. Capture scheduler, artifact, snapshot-tool, and restore evidence.
   For harness-internal entities, also run `harness-mining alignment` to
   capture the missing-state sequence as part of the restore evidence.
3. Distinguish artifact existence from restore proof.
4. Persist `backup_coverage`, `restore_evidence`, `rpo_gap`, and `dr_runbook`.
5. Hand real backup mutations to `change_lifecycle`.

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

Recovery assurance usually remains `prepared` unless a real restore verification or assurance action actually ran.

## Completion Gate

Do not finish the reply until all of the following are true:

- all seven headings are present
- artifact existence and restore proof are clearly separated
- unknown RPO or restore confidence stays explicit
- open recovery work becomes a durable next slice instead of an implicit promise

## Guardrails

- No destructive restore on live paths.
- Partial evidence must stay partial.
- State unknown RPO/RTO openly.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/recovery-checks.md](references/recovery-checks.md)
- [references/assurance-rules.md](references/assurance-rules.md)
