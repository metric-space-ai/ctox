# Change Lifecycle Rules

Keep this skill narrow:

- use `incident_response` when the problem is live stabilization first
- use `change_lifecycle` when the task is a deliberate state change or a dry-run change plan
- use `reliability_ops` after a change when the main question is post-change health

Conservative interpretation:

- a successful dry-run or pre-change snapshot is enough to persist a `change_plan`
- do not persist an executed `change_result` unless the agent actually performed the change and verified it
- rollback readiness is real only when there is a named rollback artifact or command
