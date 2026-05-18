# Recovery Assurance Helper Scripts

These scripts are open helper resources.

- `scripts/recovery_collect.py`
  - captures scheduler, backup artifact, snapshot tool, and restore metadata evidence
- `scripts/recovery_capture_run.py`
  - helper script that captures and persists one assurance run
- `scripts/recovery_store.py`
  - persists raw captures or an agent-authored graph into CTOX durable knowledge
- `scripts/recovery_query.py`
  - summarizes or exports recovery_assurance state
- `scripts/recovery_bootstrap.py`
  - conservative fallback that proposes `backup_coverage`, `restore_evidence`, `rpo_gap`, and `dr_runbook`

Use them as helpers, not as hidden authority.
