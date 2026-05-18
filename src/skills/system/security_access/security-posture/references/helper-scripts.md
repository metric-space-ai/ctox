# Security Posture Helper Scripts

These scripts are open helper resources.

- `scripts/security_collect.py`
  - captures accounts, listeners, firewall, certificate, permission, and hardening evidence
- `scripts/security_capture_run.py`
  - helper script that captures and persists one security review run
- `scripts/security_store.py`
  - persists raw captures or an agent-authored graph into CTOX durable knowledge
- `scripts/security_query.py`
  - summarizes or exports security_posture state
- `scripts/security_bootstrap.py`
  - conservative fallback that proposes a `compliance_snapshot`, `security_finding`, and `remediation_plan`

Read them before trusting them. Patch or bypass them when the posture question is more specific than the helper expects.
