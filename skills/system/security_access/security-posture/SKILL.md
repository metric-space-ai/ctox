---
name: security-posture
description: Audit host and service security posture through concrete admin evidence such as users and groups, sudo, listening sockets, firewall state, certificate expiry, secret exposure, package or vulnerability posture, and config drift. Use when CTOX needs to inspect rights, certificates, network exposure, secret handling, or basic hardening state before recommending or applying narrow fixes.
cluster: security_access
---

# Security Posture

For CTOX mission work, security findings become durable knowledge only when they are reflected in SQLite-backed runtime state, ticket knowledge, continuity, or verification records. Standalone notes do not count as durable knowledge by themselves.

Use this skill for exposure, privilege, certificate, secret, firewall, and service-hardening questions.

Do not use it for generic health review or broad inventory:

- use `discovery_graph` when scope is unclear
- use `reliability_ops` for health and saturation
- use `change_lifecycle` for actual remediation that mutates auth, firewall, TLS, or config state

This skill uses the shared SQLite kernel via `skill_key=security_posture`.

## Operating Model

Preferred helper scripts under `scripts/`:

- `security_collect.py`
- `security_capture_run.py`
- `security_store.py`
- `security_query.py`
- `security_bootstrap.py`

These scripts are open helper resources. Read them before relying on them in a tricky case.

## Tool Contracts

- `security.capture_raw`
- `security.store_capture`
- `security.store_graph`
- `security.query`
- `security.bootstrap_findings`

## Workflow

1. Define the security surface.
2. Capture raw evidence for accounts, listeners, firewall, certs, permissions, and hardening.
3. Read the raw output and tie every finding to exact evidence.
4. Persist a `compliance_snapshot`, concrete `security_finding` rows, and a `remediation_plan`.
5. Hand real mutations to `change_lifecycle`.

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

Security review usually ends in `proposed` or `prepared`. Do not imply hardening was applied unless a real mutation happened and was verified.

## Completion Gate

Do not finish the reply until all of the following are true:

- all seven headings are present
- every live finding in `Current Findings` is tied to concrete evidence
- any mutation request is clearly separated into `Escalation` or handed off to `change_lifecycle`
- if remediation work remains open, a durable next slice exists instead of vague prose

## Guardrails

- Posture is not exploitability.
- No blind privilege revokes or secret rotation.
- Prefer exact paths, users, ports, and units.

## Resources

- [references/helper-scripts.md](references/helper-scripts.md)
- [references/security-checks.md](references/security-checks.md)
- [references/finding-rules.md](references/finding-rules.md)
