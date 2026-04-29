---
name: stateful-product-from-scratch
description: Build or review new software products with UI, database, and agentic/AI automation as state-driven products instead of mockups. Use for greenfield CRM, ticketing, workflow, marketplace, dashboard, internal tool, or automation-heavy apps where objects move through states, gates, transitions, persisted runs, logs, progress, chat, and audit.
metadata:
  short-description: Build UI+DB+AI software as state-driven products
---

# Stateful Product From Scratch

Use this skill when building or reviewing a software product that has:

- a UI
- durable data
- workflow states
- human or agent transitions
- AI/agent automation
- operational review requirements

The default failure mode this skill prevents is a visually plausible mockup with fake frontend state, no durable transitions, no agent tool contract, and no evidence that the main user flow actually works.

## Core Rule

If an AI automation moves work between states, these are product core, not extras:

- `State`
- `Gate`
- `TransitionRun`
- `AgentTool`
- `Log`
- `Progress`
- `Chat`
- `Audit`
- `UIStatus`

The central loop must be explicit:

```text
object exists in state A
gate checks prerequisites for state B
user or agent starts transition
transition runs visibly and persistently
agent works through CLI/API/tooling
logs, progress, chat, and result are stored
object reaches state B or blocks/fails with evidence
```

## Worker Workflow

1. Define the product kernel before UI:
   - central object
   - states
   - gates
   - transitions
   - daily actions
   - main work surface
   - durable data model
2. Implement backend state machine before visual polish:
   - persist current and target state
   - validate allowed transitions server-side
   - persist readiness, blockers, active run, progress, start/end times
3. Build database schema, migrations, seed data, repository/services before claiming UI completion.
4. Add an agent interface:
   - CLI or API
   - JSON in/out
   - non-interactive commands
   - full context read path
   - persisted progress/log/message/complete/fail paths
5. Build UI around work, not decoration:
   - Kanban or equivalent main work surface
   - Todo/next-action surface
   - detail drawer
   - status, blockers, progress, latest log line
6. Verify the main flow:
   - DB smoke test for writes
   - build/typecheck
   - browser automation for critical flow
   - screenshots or QA artifacts

## Review Workflow

When reviewing a slice that claims a stateful product feature, fail it unless there is evidence for all relevant gates:

1. Durable model exists for the central object, states, gates, transitions, and transition runs.
2. UI reads from durable state, not frontend arrays or disconnected demo state.
3. UI writes through backend/repository/service mutations.
4. State transitions are backend-validated.
5. Agent work has a CLI/API/tool contract and does not require UI scraping.
6. Progress, logs, chat/messages, blockers, and final result persist.
7. Failure/blocker states are visible in UI and stored.
8. Main workflow has browser QA.
9. Mutations have DB smoke coverage.
10. Build/typecheck are green or the blocker is explicit.

Classify review findings as:

- `rewrite`: wording, labels, copy, register, visual terminology.
- `rework`: missing durable state, fake frontend state, absent tool contract, unvalidated transitions, missing tests, missing persisted automation evidence.

Default to `rework` when in doubt.

## Minimum Acceptance Contract

A feature is not done until:

- schema/migration exists, or an explicit durable storage substitute is documented
- seed data is plausible for the domain
- repository/service mutations exist
- UI reads durable data
- UI writes durable data
- agent CLI/API can read context and write progress/result
- transition run stores status, progress, log, chat/message, input/output
- blocked/failed states are persistent and visible
- browser test covers the main flow
- DB smoke test covers writes
- build/typecheck pass or are explicitly blocked with repair steps

## References

Read [references/stateful-product-checklist.md](references/stateful-product-checklist.md) when:

- starting a greenfield product
- reviewing a product slice with UI+DB+AI automation
- a task looks like it is drifting into mockup/demo-state
- a reviewer needs a concrete checklist for fail/pass evidence
