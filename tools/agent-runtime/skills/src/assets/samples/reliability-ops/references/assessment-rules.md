# Reliability Assessment Rules

Use raw evidence first. Prefer a missing assessment over an invented one.

## When To Use This Skill

Use `reliability_ops` when the technical scope is already mostly known and the question is:

- healthy or degraded?
- saturated or not?
- failing or not?
- what is the next low-risk diagnostic or remediation step?

Use `discovery_graph` first when the scope itself is unclear.

## Conservative Assessment Rules

- `health_assessment`
  - summarizes current host or service state
  - should remain conservative and evidence-backed
- `resource_pressure`
  - only emit when CPU, memory, disk, IO, or GPU pressure is visible from real captures
- `anomaly`
  - use for concrete failed units, endpoint failures, repeated journal errors, or strong saturation signals
- `remediation_suggestion`
  - only suggest bounded, low-risk next steps by default

## Preferred Relations

- `health_assessment -> assesses -> host`
- `resource_pressure -> observed_on -> host`
- `anomaly -> affects -> host|systemd_unit|endpoint_check`
- `remediation_suggestion -> suggests -> health_assessment`

## Guardrail

Do not turn a vague symptom into a root cause unless the captures support it.

## Reporting Guardrail

When the task is about setting up monitoring, escalation, or self-heal boundaries:

- clearly say whether the result is only proposed, prepared, executed, or blocked
- state whether monitoring is active or not active
- keep persistence details secondary to the operator-facing outcome
