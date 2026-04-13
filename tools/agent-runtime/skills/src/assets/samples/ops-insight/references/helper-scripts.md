# Ops Insight Helper Scripts

These scripts are open helper resources.

- `scripts/ops_collect.py`
  - captures current CTOX state, a compact host brief, and optional shared-kernel summaries
- `scripts/ops_capture_run.py`
  - convenience wrapper that captures and persists one ops insight run
- `scripts/ops_store.py`
  - persists raw captures or an agent-authored graph into the shared SQLite kernel
- `scripts/ops_query.py`
  - summarizes or exports ops_insight state
- `scripts/ops_bootstrap.py`
  - conservative fallback that proposes a `scorecard`, `decision_brief`, `priority_backlog`, and `dashboard_view`

Use them as helpers, not as a substitute for judgment.
