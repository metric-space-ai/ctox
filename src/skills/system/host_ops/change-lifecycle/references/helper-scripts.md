# Change Lifecycle Helper Scripts

These scripts are open helper resources.

- `scripts/change_collect.py`
  - captures current state, diff, and verification evidence
- `scripts/change_capture_run.py`
  - helper script that captures and persists one dry-run change work step
- `scripts/change_store.py`
  - persists raw captures or an agent-authored graph into CTOX durable knowledge
- `scripts/change_query.py`
  - summarizes or exports change_lifecycle state
- `scripts/change_bootstrap.py`
  - conservative fallback that proposes a `change_request`, `change_plan`, `rollback_bundle`, and `change_result`

Read them before trusting them in a nontrivial change. Patch or bypass them when the change shape needs it.
