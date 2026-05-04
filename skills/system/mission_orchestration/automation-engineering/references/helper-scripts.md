# Automation Engineering Helper Scripts

These scripts are open helper resources.

- `scripts/automation_collect.py`
  - captures current CTOX queue/schedule/plan state and repo automation hints
- `scripts/automation_capture_run.py`
  - helper script that captures and persists one automation review run
- `scripts/automation_store.py`
  - persists raw captures or an agent-authored graph into CTOX durable knowledge
- `scripts/automation_query.py`
  - summarizes or exports automation_engineering state
- `scripts/automation_bootstrap.py`
  - conservative fallback that proposes `task_pattern`, `automation_recipe`, `workflow_version`, `test_evidence`, and `adoption_note`

Use them as helpers only. The agent still decides whether the pattern is real and what should be automated.
