# Incident Response Helper Scripts

These scripts are open helper resources, not hidden authorities.

- `scripts/incident_collect.py`
  - captures raw incident evidence
- `scripts/incident_capture_run.py`
  - helper script that captures and persists one incident run
- `scripts/incident_store.py`
  - persists raw captures or an agent-authored graph into CTOX durable knowledge
- `scripts/incident_query.py`
  - summarizes or exports incident_response state from the shared kernel
- `scripts/incident_bootstrap.py`
  - conservative fallback that proposes an `incident_case`, `hypothesis_set`, `mitigation_action`, and `status_update`

Use them when they help. Read and patch them when the case is awkward.
