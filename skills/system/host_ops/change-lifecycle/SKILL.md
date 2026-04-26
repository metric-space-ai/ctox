---
name: change-lifecycle
description: Plan and execute controlled admin changes such as patches, config edits, restarts, upgrades, and rollbacks. Use when CTOX must stage a host or service change, compare current versus target config, verify preconditions, prepare rollback, or drive a maintenance-window task from dry run through post-change verification.
cluster: host_ops
---

# Change Lifecycle

Only SQLite-backed runtime state and direct live verification count as durable operational knowledge. Workspace notes, temporary files, or prose-only summaries do not count as durable knowledge by themselves.

Use this skill when the job is a deliberate state change or a dry-run change plan.

Do not use it as the first choice for scope discovery or generic health review:

- use `discovery_graph` when the target surface is still unclear
- use `reliability_ops` when the main question is health
- use `incident_response` when fast stabilization matters more than a planned rollout

This skill uses the shared SQLite kernel via `skill_key=change_lifecycle`.

## Operating Model

Treat this skill as:

1. raw change evidence capture
2. open helper resources
3. agent-authored plan, rollback, and result interpretation

Preferred helper scripts under `scripts/`:

- `change_collect.py`
- `change_capture_run.py`
- `change_store.py`
- `change_query.py`
- `change_bootstrap.py`

They are inspectable helpers, not hidden authority. Read or patch them when the change is nontrivial.

## Tool Contracts

- `change.capture_raw`
- `change.store_capture`
- `change.store_graph`
- `change.query`
- `change.bootstrap_plan`

## Post-Change Drift Verification

Every executed change shifts the harness's behaviour distribution. Use drift
detection to make that shift visible — a "successful" change that triggers a
drift alarm is a regression candidate, not a closed change:

```sh
ctox harness-mining drift --window 1000 --threshold 5.0
```

Read `chi_squared_activity.drift_detected` and `top_drift_activities[]`.

- If `drift_detected: false` after the change has settled (≥1000 events): the
  change is behaviour-stable. Record this in `change_result.verification`.
- If `drift_detected: true` and the top drift activities involve tables you
  changed: that is the expected first-order effect of the change. Note it,
  do not alarm.
- If `drift_detected: true` and the top drift activities are unrelated to
  your change scope: that is a side effect. Treat the change as `blocked`
  and create a recovery slice — even if the primary success condition is met.

Run drift detection both **before** the change (baseline) and **after** the
change has produced ≥1000 fresh events (verification). The pre/post pair is
the change's behaviour-stability evidence.

## Workflow

1. State the change target and success condition.
2. Capture current state first.
   This includes a pre-change baseline `harness-mining drift` snapshot.
3. Read the helper scripts if the change shape is unusual.
4. Gather dry-run evidence with `change_collect.py` or `change_capture_run.py`.
5. Read the raw output, especially diffs, service state, and verification output.
6. Persist captures first, then persist a `change_request`, `config_snapshot`, `change_plan`, `rollback_bundle`, and only an executed `change_result` if a change really happened.
7. Keep the change slice narrow.
8. If rollback is unclear, stop before mutation.
9. Post-change: rerun `harness-mining drift` once ≥1000 fresh events have
   accumulated. Attach the diff to `change_result.verification`.

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

Treat dry runs, staged plans, rollback bundles, and preflight checks as `prepared`, not `executed`.

If a change started but is not yet complete, say so explicitly and leave a durable next-work record instead of saying you are "working on it" without closure.

## Completion Gate

Do not finish the reply until all of the following are true:

- all seven headings are present
- `State` explicitly reflects whether the system changed
- if `State` is `executed`, post-change verification is stated in `Current Findings`
- if the change is incomplete, a durable next slice or rollback step exists in queue or plan state
- if no safe continuation exists yet, the reply stays `blocked`

## Guardrails

- Default to dry-run.
- No broad restarts or package changes without a rollback path.
- Do not claim execution if only a plan was produced.
- Hand unresolved follow-up slices to queue or plan, not prose.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/change-commands.md](references/change-commands.md)
- [references/change-rules.md](references/change-rules.md)
