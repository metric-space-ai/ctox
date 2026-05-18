# Incident Response Triage Rules

Keep this skill narrow:

- use `discovery_graph` when the affected scope is still unclear
- use `reliability_ops` when the question is health assessment without urgent stabilization
- use `incident_response` when there is a live symptom that needs fast containment and an explicit incident trail

Conservative interpretation:

- failed service state, failing endpoint checks, repeated warning/error journal lines, or strong resource pressure are enough to open a small incident case
- prefer a low-risk suggested mitigation over an executed mitigation unless the user asked for action or the environment already permits it
- do not claim root cause if the evidence only supports symptoms and hypotheses
