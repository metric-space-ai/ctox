# Recovery Assurance Rules

Keep this skill narrow:

- use `recovery_assurance` for backup freshness, restoreability, and DR confidence
- use `change_lifecycle` when changing backup jobs or restore paths
- use `incident_response` when a real outage already exists

Conservative interpretation:

- snapshot or artifact existence is not restore proof
- successful metadata listing or isolated restore checks count as partial restore evidence
- if the tool is present but no restore check ran, keep the result partial and state the gap
