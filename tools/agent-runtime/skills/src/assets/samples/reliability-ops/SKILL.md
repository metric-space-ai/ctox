---
name: reliability-ops
description: Evaluate live service health, anomalies, capacity pressure, and host resource behavior using concrete observability commands and logs. Use when services are slow, failing, resource-bound, or need recurring health review, including CPU, memory, disk, network, or GPU investigation with htop, btop, top, vmstat, iostat, ss, journalctl, nvidia-smi, and service-specific checks.
---

# Reliability Ops

Use this skill to turn a known or mostly-known technical scope into a concrete health assessment, anomaly list, and next safe action.

Use `discovery_graph` first when the technical scope is still unclear. Use `reliability_ops` when the scope is already known enough that the question is health, saturation, degradation, or failure.

## Operating Model

This skill uses the same SQLite persistence kernel as `discovery_graph`.

The shared kernel stays:

- `discovery_run`
- `discovery_capture`
- `discovery_entity`
- `discovery_relation`
- `discovery_evidence`

The separation is done through `skill_key`:

- `discovery_graph`
- `reliability_ops`

This keeps one SQLite source of truth while letting each skill add its own collectors, entities, and relations.

## Preferred Helpers

These helper scripts are open resources under `scripts/`:

- `reliability_collect.py`
- `reliability_capture_run.py`
- `reliability_store.py`
- `reliability_query.py`
- `reliability_bootstrap.py`

Read them when the case is nontrivial. Use them when they fit. Patch or bypass them when the raw evidence requires it.

## Workflow

1. Define the symptom.
   Capture whether the problem is latency, errors, restart churn, queue growth, disk pressure, memory pressure, or total unavailability.
2. Inspect the available helpers.
   Prefer the local helper scripts, but do not treat them as hidden authority.
3. Capture raw reliability evidence.
   Use `reliability_collect.py` or `reliability_capture_run.py` to gather CPU, memory, disk, network, service, log, endpoint, and GPU state.
4. Read the raw output.
   Do not skip the raw evidence just because a helper returned JSON.
5. Separate cause classes.
   Distinguish saturation, configuration drift, dependency failure, crash-looping, slow downstreams, and external traffic spikes.
6. Prefer evidence over theory.
   Quote exact processes, ports, units, devices, error strings, or counters that justify the assessment.
7. Persist the run in the shared kernel.
   `reliability_ops` writes to the same SQLite kernel as `discovery_graph`, but with `skill_key=reliability_ops`.
8. Bootstrap only when useful.
   `reliability_bootstrap.py` may help produce a conservative first `graph.json`, but it is not the authority.
9. Keep remediation narrow.
   If a low-risk fix is obvious, state it explicitly. If not, queue or plan the next slice instead of improvising a broad change.

## Operator Feedback Contract

Your answer must always be operator-readable first and persistence-readable second.

Always make the execution state explicit:

- `proposed`
  - recommendation only
- `prepared`
  - policy, monitoring plan, thresholds, or reports were created, but nothing was activated
- `executed`
  - CTOX actually enabled monitoring, restarted something, or changed runtime behavior
- `blocked`
  - intended action could not be completed

For reliability work, answer in this order:

1. `Status`
   - `healthy`, `degraded`, or `critical`, plus one sentence why
2. `State`
   - `proposed`, `prepared`, `executed`, or `blocked`
3. `Monitoring Scope`
   - what resources or signals are covered
4. `Autonomous Action`
   - what CTOX may do itself, or what it already did itself
5. `Escalation`
   - what still requires owner approval or emergency escalation
6. `Current Findings`
   - real active issues only
7. `Next Step`
   - one clear next operator action

Use these exact markdown headings:

- `**Status**`
- `**State**`
- `**Monitoring Scope**`
- `**Autonomous Action**`
- `**Escalation**`
- `**Current Findings**`
- `**Next Step**`

Do not begin with `Persistiert`, database paths, or entity counts unless the user explicitly asked for storage details.

## Completion Gate

Do not finish a user-facing reply until all of the following are true:

- the reply contains all seven required headings
- `State` explicitly says `proposed`, `prepared`, `executed`, or `blocked`
- the reply explicitly says whether monitoring is active or not active when the task is about setup or activation
- if the work remains open, a durable next slice exists in queue or plan state instead of vague prose
- persistence details, if included at all, come after the operator-facing outcome

## Tool Contracts

Think in these canonical capabilities:

- `reliability.capture_raw`
- `reliability.store_capture`
- `reliability.store_graph`
- `reliability.query`
- `reliability.bootstrap_assessment`

The helper scripts are the current local implementations of these capabilities.

## Host And Service Signals

- CPU: `htop`, `btop`, `top`, `ps`
- Memory and swap: `free`, `vmstat`, `ps`
- Disk and filesystem: `df`, `du`, `iostat`, `findmnt`
- Network and sockets: `ss`, `ip`, `curl`
- GPU: `nvidia-smi`
- Service state: `systemctl`, `journalctl`, container logs

## CTOX Integration

- For recurring health checks, use `ctox schedule add --skill "reliability-ops"`.
- For unresolved concrete follow-up work, use `ctox queue add --skill "reliability-ops"`.
- If the scope itself is still unclear, hand the next slice to `discovery-graph`.
- If a health issue becomes user-visible or high-risk, hand the next slice to `incident-response`.

## Guardrails

- Read state before changing state.
- Do not call a symptom the root cause unless the evidence supports it.
- Distinguish transient spikes from sustained saturation.
- Prefer targeted restarts or bounded remediation only when the user asked for action or the risk is clearly low.
- If you cannot prove the service is healthy, say what remains unverified.
- Do not create a broad inventory here; if scope is unclear, switch to `discovery_graph`.

## References

Read [references/host-observability.md](references/host-observability.md) for the concrete host-level command set.
Read [references/helper-scripts.md](references/helper-scripts.md) for the local helper roles.
Read [references/assessment-rules.md](references/assessment-rules.md) for conservative interpretation rules.
Read [references/operator-report.md](references/operator-report.md) for the required response shape.
