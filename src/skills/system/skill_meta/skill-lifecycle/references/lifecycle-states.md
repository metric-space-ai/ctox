# Skill Lifecycle States

- `draft`
  - early or unstable
  - expected to change quickly
- `candidate`
  - initial tests passed
  - acceptable for bounded real work
- `promoted`
  - validated enough for normal reuse
  - should be part of the trusted working set
- `deprecated`
  - still documented for continuity
  - no longer preferred
- `retired`
  - superseded or intentionally removed from active use

Promotion guidance:

- `draft -> candidate`
  - requires at least one direct validation artifact
- `candidate -> promoted`
  - requires repeated success or broader review confidence
- `promoted -> deprecated`
  - use when a better replacement exists or the old behavior is no longer desirable
