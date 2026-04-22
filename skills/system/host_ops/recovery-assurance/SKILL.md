---
name: recovery-assurance
description: Verify backup and restore readiness for hosts and services. Use when CTOX needs to inspect snapshot or backup coverage, retention, restore procedures, database dump validity, off-host copies, or disaster recovery confidence after a critical change or as a recurring assurance check.
cluster: host_ops
---

# Recovery Assurance

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

## Workflow

1. Name what must be recoverable.
2. Capture scheduler, artifact, snapshot-tool, and restore evidence.
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
