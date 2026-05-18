# Reliability Ops Helper Scripts

These scripts are open helper resources, not hidden authorities.

- `scripts/reliability_collect.py`
  - raw host, service, endpoint, and GPU signal capture
- `scripts/reliability_capture_run.py`
  - helper script for one persisted raw reliability sweep
- `scripts/reliability_store.py`
  - shared CTOX persistence helper using the same 5-table kernel as `discovery_graph`
- `scripts/reliability_query.py`
  - filtered summary/export for `skill_key=reliability_ops`
- `scripts/reliability_bootstrap.py`
  - conservative bootstrap assessment from stored captures

Use them when they fit. Read them when the case is unclear. Patch them if they are close but wrong.
