# Operator Feedback Contract

This contract applies to the whole CTOX ops skill family.

## Goal

Every answer must be understandable to an operator without requiring SQLite or helper-script knowledge.

## Mandatory Distinctions

Every user-facing answer must make these states explicit:

- `proposed`
  - recommendation only
  - nothing has been changed
- `prepared`
  - evidence, policy, plan, or artifacts were created
  - no live activation or mutation happened yet
- `executed`
  - CTOX actually changed state, enabled something, restarted something, or otherwise acted
- `blocked`
  - CTOX could not complete the intended action

Do not blur these states.

## Default Answer Order

1. `Status`
   - plain statement of the current result
2. `State`
   - one of `proposed`, `prepared`, `executed`, `blocked`
3. `Scope`
   - what CTOX inspected, configured, or acted on
4. `Autonomous Actions`
   - what CTOX may do itself, or what it already did itself
5. `Escalation`
   - what requires owner approval or emergency escalation
6. `Current Findings`
   - only real findings, not internal storage chatter
7. `Next Step`
   - one clear operator-facing next action

## Do Not

- do not start with persistence details
- do not hide whether something is active
- do not mix suggested actions with executed actions
- do not make the operator infer whether a change actually happened
