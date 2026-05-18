# Reliability Ops Operator Report

Use this structure for operator-facing answers.

## Required Order

1. `Status`
2. `State`
3. `Monitoring Scope`
4. `Autonomous Action`
5. `Escalation`
6. `Current Findings`
7. `Next Step`

## State Meanings

- `proposed`
  - CTOX recommends a setup or action, but has not prepared or activated it
- `prepared`
  - CTOX created or persisted monitoring policy, thresholds, or escalation rules, but did not activate them
- `executed`
  - CTOX actually enabled or changed monitoring or performed a permitted low-risk action
- `blocked`
  - CTOX could not complete the intended action

## Good Example

- `Status:` Target host is degraded because swap is nearly full and three CTOX services are failing to start repeatedly.
- `State:` prepared. Monitoring thresholds and escalation tiers were stored, but not activated.
- `Monitoring Scope:` CPU, memory, swap, disk, network, services, logs, GPU.
- `Autonomous Action:` CTOX may run health checks, inspect logs, and restart one CTOX service at a time if explicitly active and within the low-risk boundary.
- `Escalation:` Reboot, package or driver changes, config rewrites, BIOS/root-auth, and destructive cleanup require escalation.
- `Current Findings:` Swap pressure; repeated failures in `cto-agent.service`, `cto-kleinhirn.service`, and `cto-jami-daemon.service`.
- `Next Step:` confirm activation if CTOX should start enforcing this monitoring policy.

## Bad Example

- leading with database writes
- mixing planned and executed actions in one sentence
- making the operator infer whether monitoring is active
