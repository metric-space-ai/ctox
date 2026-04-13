# Automation Engineering Rules

Keep this skill narrow:

- use `automation_engineering` when repeated operational work should become a repo script, queued slice, or scheduled CTOX task
- use `change_lifecycle` for the actual rollout of a new automation artifact
- use `ops_insight` when the task is only a report, not an automation candidate

Conservative interpretation:

- repeated queue/schedule evidence or repeated repo command patterns are enough to draft an `automation_recipe`
- do not create a hidden daemon or external loop
- keep automation proposals explicit about dry-run, parameters, and rollback
