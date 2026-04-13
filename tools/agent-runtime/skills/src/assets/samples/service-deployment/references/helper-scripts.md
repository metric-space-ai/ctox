# Helper Scripts

These scripts are open helper resources:

- `deployment_collect.py`
  - raw preflight collectors for package managers, ports, and service presence
- `deployment_capture_run.py`
  - convenience wrapper for one preflight sweep
- `deployment_store.py`
  - shared SQLite persistence wrapper using `skill_key=service_deployment`
- `deployment_bootstrap.py`
  - conservative bootstrap graph for deployment runs

Inspect or patch them when the host or service shape is unusual.
