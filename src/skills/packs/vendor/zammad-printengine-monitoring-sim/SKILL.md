---
name: zammad-printengine-monitoring-sim
description: Use when CTOX needs a bounded execution-capable monitoring source for the zammad Printengine alert family (`HIST-011`).
metadata:
  short-description: Simulated monitoring source for zammad Printengine alerts
cluster: vendor
---

# Zammad Printengine Monitoring Sim

This skill provides a small, explicit execution source for the `HIST-011` zammad
family:

- `** PROBLEM Service Alert: Filialserver 133IG/Prestige Printengine is CRITICAL **`

It exists so the first zammad-specific execution supplement can be built from a
real tool path and a real evidence file, not from ticket history alone.

## Source Files

- [printengine_monitoring_playbook.md](references/printengine_monitoring_playbook.md)
- [printengine_alert_snapshot.json](references/printengine_alert_snapshot.json)

## Tool

```sh
python3 skills/packs/vendor/zammad-printengine-monitoring-sim/scripts/check_printengine_alert.py \
  --snapshot skills/packs/vendor/zammad-printengine-monitoring-sim/references/printengine_alert_snapshot.json \
  --service prestige_printengine
```

## Intended Use

1. Check the current alert state from the simulated monitoring snapshot.
2. Ingest the same snapshot into the ticket knowledge plane with
   `ctox ticket monitoring-ingest`.
3. Build the `HIST-011` execution supplement from this evidence.
4. Promote the enriched runbook item only if the supplement is explicit.

## Boundary

This is a testbed execution source for zammad onboarding. It confirms the
monitoring check and the writeback boundary for the alert family. It does not
claim to fix the underlying Printengine incident itself.
