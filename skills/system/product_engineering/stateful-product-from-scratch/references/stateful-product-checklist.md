# Stateful Product Checklist

## One-Shot Checklist

### 1. Product Kernel First

- What is the central object? Example: lead, deal, ticket, order, document.
- Which states exist? Example: discovery, qualified, proposal, won.
- Which daily actions matter? Example: todo, drag, edit, start transition.
- Which view is the main work surface? Usually Kanban + Todo + detail drawer.
- Which master data must be editable? Accounts, contacts, leads, users, settings.
- Which data is really persistent, not demo state?

### 2. State Machine Before UI

- Every column, status, and transition must be modeled in the backend.
- Tables need clear fields:
  - `current_state_id`
  - `next_state_id`
  - `transition_readiness`
  - `transition_blockers`
  - `active_transition_run_id`
  - `transition_progress`
  - `transition_started_at`
  - `transition_finished_at`
- Gates belong to target/transition logic, not just UI text.
- A transition must not only happen visually.
- Drag and drop must trigger only allowed state changes.
- Progress belongs in the target state once the object has moved there.
- Blockers must be structurally stored.

### 3. Database First

- Schema and migrations before UI completion.
- Seed data must be domain-plausible.
- No important product data in frontend arrays.
- Repository/service layer for all mutations.
- Audit-relevant mutations store:
  - `created_by`
  - `updated_by`
  - `owner_id`
  - `organization_id`
  - `created_at`
  - `updated_at`
  - `deleted_at`
- Transition runs store snapshots:
  - gate criteria snapshot
  - agent prompt snapshot
  - agent todo snapshot
  - log
  - progress
  - status
  - input/output
- No automation logic only in the browser.

### 4. Native Agent Interface

- The agent needs tooling, not just a prompt.
- Minimum CLI/API:
  - `list-work`
  - `get-context`
  - `start-transition`
  - `write-progress`
  - `write-log`
  - `write-message`
  - `complete-transition`
  - `fail-transition`
  - `set-readiness`
- Commands read and write JSON.
- No interactive CLI mode as the only interface.
- Agent must not scrape the UI.
- Agent context includes:
  - record
  - current state
  - target state
  - gate prerequisites
  - agent todos
  - agent prompt
  - related account/contact
  - timeline
  - messages
  - existing blockers
- Agent result persists:
  - progress
  - log
  - chat/messages
  - new blockers
  - completion status
  - next readiness

### 5. UI Explains State

- Main work surface visible immediately.
- Kanban and Todo are work surfaces, not decoration.
- Card content is decision-relevant:
  - account
  - object name
  - contact
  - value or score
  - next action
  - due date
  - gate status
  - blocker or ready-to-move
  - running progress
- Detail view is a drawer, not a scroll trap below the page.
- Todo list is a side drawer or equivalent persistent work area.
- Stage configuration lives at the stage/column.
- Gate, agent prompt, and todos are edited where they act.
- Card click toggles the drawer and preserves board scroll.
- Drag and drop:
  - only next allowed state
  - only when gate ready
  - clear error when blocked
  - starts transition and shows object in target state with progress

### 6. Automation UX

- Each stage shows:
  - exit criteria
  - agent todos
  - ready count
  - running count
- Each card shows:
  - blocked / ready / running / completed
  - ready to move to ...
  - blocker reason
  - progress bar
  - latest log line
- Detail drawer shows:
  - gate prerequisites
  - agent prompt
  - agent todos
  - transition run
  - progress
  - log
  - chat/messages
  - start button
  - fail/complete result
- User can answer:
  - Why can I start?
  - Why can I not start?
  - What is the agent doing?
  - What is missing?
  - When is the state complete?

### 7. Design Rules

- Form follows function.
- No decorative cards, badges, gradients, or icons without a job.
- Every element must improve orientation, decision, or action.
- Use few visual layers:
  - navigation
  - work surface
  - context drawer
  - status/action
- Prioritize typography and spacing for readability.
- Think small screens early.
- No dashboard theater.
- No large hero areas in internal tools.
- Avoid too many borders, shadows, colors, pills, and huge headings.
- Status colors:
  - ready = green
  - blocked/failed = red
  - running = blue/teal
  - overdue = red
  - neutral = gray

## Go

- Backend state machine first.
- Persisted gates, prompts, todos, and runs.
- Agent over CLI/API with JSON contract.
- UI shows actual state.
- Kanban + Todo as central work surface.
- Detail and Todo drawers instead of scroll traps.
- Browser automation after UI iterations.
- DB smoke tests for each mutation.
- Build/typecheck before completion.
- Domain-plausible seeds.
- Logs and timeline for every automation.
- i18n early when bilingual product is required.
- Responsive layout as a requirement.

## No-Go

- Pretty UI first, data later.
- Frontend arrays as product logic.
- Agent only as prompt text.
- Client-only transition simulation.
- Progress bar without persistent run.
- Drag and drop without backend validation.
- Gates only as descriptions.
- Automation logs only in `console.log`.
- Modals under the board instead of overlay/drawer.
- Header, hero, or branding larger than work surface.
- Generic SaaS cards without function.
- Implausible dummy data.
- No browser QA.
- No migration/build check.
- No cleanup strategy for DB-writing tests.

## Definition Of Done

- Data model and migration exist.
- Seed data is plausible.
- Repository/service mutations exist.
- UI reads from durable storage.
- UI writes to durable storage.
- Agent can read and write through CLI/API.
- Transition run has progress, log, chat/messages, and status.
- Error states are visible and persistent.
- Browser test covers the main flow.
- DB smoke test covers writes.
- Typecheck passes.
- Build passes.
- Screenshots/QA artifacts exist.

## One-Shot Prompt

```text
Do not build a mockup. Build a state-driven product with real database, UI, and agent automation.

Required:
1. Define the central object, states, gates, and transitions first.
2. Create schema, migrations, and domain-plausible seeds.
3. Implement repository/domain services for all mutations.
4. Build the UI around the main work surface: Kanban + Todo + detail drawer.
5. Every stage has gate criteria, agent todos, and agent prompt.
6. Every card shows status, next action, due date, blocker/ready/running, and progress.
7. Drag and drop may only start the next allowed transition and must be backend-validated.
8. The agent gets a CLI/API tool with JSON contract:
   list-work, context, start, progress, log, message, complete, fail, set-readiness.
9. Progress, logs, messages, prompts, todos, and results persist.
10. No fake frontend state, no decorative UI without function.
11. Design is minimal, readable, functional, and not theatrical.
12. Verify with typecheck, build, DB smoke test, and browser automation.
13. Return concrete artifacts, tested commands, and known gaps.
```
