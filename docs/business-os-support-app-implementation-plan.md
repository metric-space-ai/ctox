# Business OS Support App Implementation Plan

Status: Phase 0 through Phase 5 implemented and production-ready verified on
2026-06-17. Static, schema, native, command-path, and live browser validation
were run against the real CTOX Business OS shell, WebRTC/RxDB replication, and
typed Support command handlers.

This plan describes a CTOX-native Support app that recreates the operator
experience of an omnichannel support desk without porting Chatwoot's Rails,
REST, Redis, ActionCable, or Vue architecture.

The app must fit the current CTOX Business OS architecture:

- Business data stays on CTOX Sync Engine / RxDB / WebRTC.
- Browser actions go through `ctx.commandBus.dispatch`.
- Native Rust validates commands, mutates canonical state, and projects records
  back to Business OS collections.
- Agent work goes through Business Chat / `business_os.chat.task`, durable
  `ctox_queue_tasks`, the CTOX service harness, review/outcome gates, and typed
  Business OS writeback commands.

## Current System Boundaries

The Support app must not replace existing modules.

| Existing surface | Ownership | Support app relationship |
| --- | --- | --- |
| `conversations` | Read-only audit surface over `communication_accounts`, `communication_threads`, `communication_messages` | Support consumes raw channel threads/messages and turns them into operator work. |
| `tickets` | Native CTOX ticket operations over `ctox_ticket_*` projections | Support can create/link ticket cases, but Tickets remains the evidence/control surface. |
| `customers` | CRM master over `customer_*` collections | Support links identities to customers/contacts and can request customer updates via `customers.*` commands. |
| `business_chats` / `ctox_queue_tasks` | Harness-visible chat/task projections | Support uses this for Agent/CTOX work, not a private bot runner. |
| `business_commands` | Typed command bus | Support mutations and Agent writebacks are commands, never direct browser writes. |

## Product Goal

Build a first-class Business OS Support Desk:

- omnichannel queue over email, WhatsApp, Jami, Teams, and future channels
- stable workbench with list, timeline, and context panes
- support statuses, priorities, assignment, snooze, SLA, labels, notes, macros
- deep links into Customers and Tickets
- CTOX Agent assistance for summaries, classifications, draft replies, and
  next-action suggestions
- human-controlled external communication with approval/writeback evidence

The first release is packaged as a source-bundled Store module with
`default_installed` false so it satisfies the Business OS app validator, while
the required state transitions still live in native Rust `support.*` command
handlers and first-class RxDB collections.

## Chatwoot Concepts Recast For CTOX

| Chatwoot concept | CTOX Support equivalent |
| --- | --- |
| `Inbox` | `support_inboxes` queue/channel policy |
| `ContactInbox` | `support_identity_links` external identity to customer/contact link |
| `Conversation` | `support_conversations` operator aggregate |
| `Message` | read-only `communication_messages` plus Support events/notes |
| Assignment policy | `support_assignment_policies` plus atomic SQLite claim |
| Macros | `support_macros`, executed as typed Support commands |
| Automation rules | `support_automation_rules`, closed action enum |
| SLA policy / applied SLA | `support_sla_policies`, `support_applied_slas`, `support_sla_events` |
| Reporting events | `support_reporting_events` plus rebuildable rollups |
| Bot / AI | `business_os.chat.task` + Harness + typed writeback commands |

## App UX

### Layout

Use the existing Business OS full-workspace pattern.

- Left pane: queue list and filters.
- Center pane: selected support timeline and composer.
- Right pane: customer/contact card, related tickets, SLA, Agent tasks,
  macros, and activity.

Unlike generated App Creator modules, Support can justify a persistent third
pane because operators need customer identity, ticket, SLA, and CTOX suggestion
context visible while reading and replying.

### Queue Filters

Required first release filters:

- `Mine`
- `Unassigned`
- `All open`
- `Needs reply`
- `SLA risk`
- `Snoozed`
- `Agent drafts`
- channel
- inbox/team
- priority
- label
- customer/contact

The filter model should be data-driven so saved views can reuse it.

### Timeline

Timeline entries are merged client-side from:

- `communication_messages` for inbound/outbound channel messages
- `support_notes` for internal notes
- `support_conversation_events` for status, assignment, priority, SLA, macro,
  Agent, and ticket-link events
- selected `ctox_ticket_events` and `ctox_ticket_writebacks`
- `business_commands` / `ctox_queue_tasks` for active Agent tasks

The Support app does not write communication messages directly.

### Composer

Modes:

- `Reply`
- `Internal note`
- `Ask CTOX`
- `Apply macro`
- `Create/link ticket`
- `Resolve`

External replies must be guarded:

- If channel policy allows direct human send, `support.reply.send` can execute.
- If policy requires approval, `support.reply.send` creates a pending approval
  record and does not send.
- Agent-generated drafts are never sent directly; they must be applied by a
  human and then pass the same reply policy.

## Support-Owned Collections

Add first-class collections to the Business OS schema contract. The app's
`schema.js` exports `supportCollections` for Support-owned schemas. Its
`collections` export also imports the canonical `business_chats` schema from
`modules/ctox/schema.js` because the module manifest declares `business_chats`
for Harness/Business Chat readback; that foreign schema must never be copied or
forked. Cross-module reads should reference existing owner schemas or a new
shell dependency mechanism, not duplicate foreign schemas.

### MVP Collections

- `support_inboxes`
  - queue/channel policy, channel filters, team, assignment policy, SLA policy
- `support_conversations`
  - operator aggregate
  - fields: `id`, `primary_thread_key`, `status`, `priority`, `assignee_id`,
    `team_id`, `customer_account_id`, `customer_contact_id`, `ticket_case_id`,
    `waiting_since_ms`, `snoozed_until_ms`, `last_activity_at_ms`,
    `last_message_key`, `unread_count`, `created_at_ms`, `updated_at_ms`,
    `is_deleted`
- `support_thread_links`
  - links one support conversation to one or more `communication_threads`
- `support_identity_links`
  - external identity resolution: channel/account/thread/source address to
    `customer_contact_id` and optionally `customer_account_id`
- `support_notes`
  - internal notes, visible only in Support/related activity
- `support_conversation_events`
  - append-only support activity log for status, assignment, priority, macro,
    Agent, SLA, ticket-link, and resolve events
- `support_labels`
- `support_label_assignments`
- `support_views`
- `support_view_filters`

### Phase 2 Collections

- `support_assignment_policies`
- `support_assignment_events`
- `support_macros`
- `support_automation_rules`
- `support_sla_policies`
- `support_applied_slas`
- `support_sla_events`
- `support_agent_requests`
- `support_agent_suggestions`
- `support_reporting_events`
- `support_reporting_rollups`

## Command Contract

All app mutations use `ctx.commandBus.dispatch`. Command documents must set both
`type` and `command_type` to the same value.

### MVP Support Commands

- `support.conversation.open_from_thread`
- `support.conversation.claim`
- `support.conversation.assign`
- `support.conversation.status`
- `support.conversation.priority`
- `support.conversation.snooze`
- `support.conversation.resolve`
- `support.conversation.reopen`
- `support.identity.link`
- `support.note.create`
- `support.ticket.link`
- `support.ticket.create_from_conversation`
- `support.reply.draft`
- `support.reply.send`

### Agent Writeback Commands

Agent work must not be modelled as private app state. It is normal CTOX Harness
work:

1. Support app dispatches `business_os.chat.task` with `module: "support"`.
2. Native Business OS accepts the command and creates a durable queue task.
3. CTOX service leases and runs the task through the normal Harness.
4. The model response is reviewed/gated by the service.
5. Chat-only results are written back to `business_chats`.
6. Structured Support results must be written through typed commands.

Add these typed commands:

- `support.agent.writeback`
  - called by the Agent through `ctox business-os commands dispatch` or the
    Business OS MCP Channel once Support actions are registered
  - writes `support_agent_suggestions`
  - requires `source_command_id`, `task_id`, `conversation_id`,
    `suggestion_kind`, `payload`, `confidence`, `required_human_action`
- `support.agent.apply_suggestion`
  - human action from the Support app
  - converts a suggestion into a draft, status change, note, label, ticket link,
    customer update proposal, or macro execution
- `support.agent.reject_suggestion`
  - records rejection reason and closes the suggestion

`support.agent.writeback` is the critical correction: Agent output cannot be
only assistant prose in Business Chat if the Support app needs structured state.

### Example Agent Task Command

The app dispatches this through `ctx.commandBus.dispatch`, not through direct
`business_commands` writes and not through a window-level event.

```json
{
  "id": "cmd_support_agent_summary_<uuid>",
  "module": "support",
  "type": "business_os.chat.task",
  "command_type": "business_os.chat.task",
  "record_id": "support_conv_123",
  "inbound_channel": "support",
  "payload": {
    "title": "Support summary · ACME renewal issue",
    "instruction": "Summarize the selected support conversation and propose the next action. Write structured results only through support.agent.writeback.",
    "prompt": "Summarize the customer problem, current status, risk, and next action.",
    "user_message": "Summarize this support case.",
    "mode": "data",
    "target": "data",
    "thread_key": "business-os/support/support_conv_123",
    "required_skills": ["business-os-support-workflow"],
    "record_snapshot": {
      "support_conversation": {},
      "customer": {},
      "recent_messages": [],
      "related_tickets": []
    },
    "writeback_contract": {
      "command_type": "support.agent.writeback",
      "collection": "support_agent_suggestions",
      "record_id": "support_conv_123",
      "allowed_suggestion_kinds": ["summary", "draft_reply", "classification", "next_action"],
      "source_collection": "support_conversations"
    },
    "response_channel": "business_os_chat",
    "outbound_channel": "business_os_chat"
  },
  "client_context": {
    "source": "support-agent-task",
    "module": "support",
    "surface": "support.ask_ctox",
    "record_type": "support_conversation",
    "record_id": "support_conv_123"
  }
}
```

## Correct Agent/Harness Integration

The Support app is a task producer and reviewer surface. It does not run agents.

### What happens after `business_os.chat.task`

Current CTOX flow:

- `src/apps/business-os/shared/business-chat.js` builds a command with
  `business_os.chat.task`, `thread_key`, attachments, `required_skills`,
  payload, and client context.
- `src/core/business_os/store.rs::record_command` persists the command and calls
  `create_ctox_queue_task`.
- `create_ctox_queue_task` materializes attachments, builds a bounded prompt,
  copies required skills into the prompt, and creates a durable queue task.
- `src/core/service/service.rs` treats `business_os.chat.task` queue jobs as
  Business OS chat jobs.
- The service runs the normal worker slice, review, outcome handling, and queue
  terminalization.
- `complete_business_command_from_queue_reply` completes chat/document commands
  from the queue result and projects `business_commands`, `ctox_queue_tasks`,
  and `business_chats`.

Support must use that path.

### Shell API Adjustment

Existing modules can use the legacy `ctox-business-os-chat-submit` event, but
new Support code should not depend on a window-level event.

Add a first-class shell context API:

```js
ctx.businessChat.submitTask({
  module: 'support',
  recordId,
  title,
  instruction,
  prompt,
  payload,
  clientContext,
  openChat: true
});
```

Internally this can reuse the shared Business Chat implementation. The app
should still be able to dispatch `business_os.chat.task` directly through
`ctx.commandBus.dispatch` for non-chat background tasks.

### MCP Channel Adjustment

Add Support action descriptors in `src/core/business_os/mcp_channel.rs`:

- `support.agent.writeback`
- `support.agent.apply_suggestion`
- `support.agent.reject_suggestion`

`business_os.execute_action` should enqueue typed Support commands without
wrapping the Support payload in a generic action envelope. External effects
remain blocked in MCP Channel v1. Any outbound customer message must come from
an explicitly approved human action in the Support app.

### Agent Skill

Add a Support-specific skill, for example `business-os-support-workflow`, whose
contract says:

- read support context from the prompt and Business OS MCP tools
- do not send external messages
- do not mutate Customers/Tickets directly unless the writeback contract
  explicitly asks for that typed command
- write structured support results via `support.agent.writeback`
- include `source_command_id` and `task_id`
- keep the final assistant reply short because structured state is the source
  of truth

## Native Backend Changes

### Rust Modules

Add:

- `src/core/business_os/support.rs`
  - command allowlist
  - payload structs
  - idempotency guards
  - SQLite mutations
  - projection helpers
  - SLA/assignment calculations

Wire into:

- `src/core/business_os/mod.rs`
- `src/core/business_os/store.rs`
  - `is_support_active_command`
  - `handle_support_command`
  - post-command Support projection refresh
- `src/core/business_os/rxdb_peer.rs`
  - after accepting `support.*`, refresh Support projections like
    `ctox.ticket.*` already refreshes ticket projections
- `src/core/business_os/business_os_schema_contract.json`
- `src/core/business_os/business_os_schema_hashes.json`
- `src/apps/business-os/rxdb/src/schema.mjs`

### Canonical Tables

Create native SQLite tables for Support state. Use the same durable-state-first
rules as Customers/Tickets:

- no transient in-memory queue state as source of truth
- every user action is reconstructable from command, event, and projection rows
- projections are rebuildable
- external channel sends are auditable

Tables should mirror Support collections closely:

- `support_inboxes`
- `support_conversations`
- `support_thread_links`
- `support_identity_links`
- `support_notes`
- `support_conversation_events`
- `support_labels`
- `support_label_assignments`
- `support_views`
- `support_view_filters`
- later phase tables for assignment, macros, SLA, Agent suggestions, reporting

### Intake Projector

Add a Support intake projector:

- reads `communication_threads` and `communication_messages`
- maps eligible threads into `support_conversations`
- links threads through `support_thread_links`
- updates `last_activity_at_ms`, `last_message_key`, `unread_count`,
  `waiting_since_ms`
- applies `support_inboxes` routing policy
- optionally creates `support_identity_links` candidates

The projector must not mutate `communication_*`. It only derives Support state.

### Customers Integration

Support can link to existing customers and contacts:

- `support.identity.link` writes Support identity link records.
- If a new contact/account is needed, Support dispatches `customers.contact.create`
  or `customers.account.create` rather than writing customer collections.
- Customer updates proposed by an Agent use `support.agent.writeback` first and
  `customers.*` only after human approval.

### Tickets Integration

Tickets remains the case/evidence system:

- `support.ticket.link` stores a Support-side link to an existing ticket case.
- `support.ticket.create_from_conversation` should call or reuse the native
  `ctox.ticket.local.create` path and store the relation once the ticket
  projection exists.
- Ticket events render in the Support timeline, but Support does not own
  `ctox_ticket_*`.

### Reply/Send Integration

First release can support draft-only replies. Direct sending requires a channel
send gateway.

Backend requirement for sending:

- `support.reply.draft` stores a draft.
- `support.reply.send` validates actor, channel, identity, approval policy,
  thread, attachments, and required fields.
- The native handler calls the existing communication/outbound send mechanism
  or creates an approval-gated outbound item.
- Result is projected as Support event plus channel writeback evidence.

Do not add an HTTP send bridge.

## Business OS App Files

Add module directory:

```text
src/apps/business-os/modules/support/
  module.json
  collections.schema.json
  schema.js
  index.html
  index.css
  index.js
  icon.svg
  locales/de.json
  locales/en.json
  support-commands.mjs
  support-reducers.mjs
  tests/support.test.mjs
```

The app should:

- load `index.html` and `index.css` in the established module pattern
- declare `business_commands`, `business_chats`, `ctox_queue_tasks`,
  Support-owned collections, and read-only dependency collections in
  `module.json`
- export only Support-owned collections in `schema.js`
- use shared shell services: `ctx.db`, `ctx.sync`, `ctx.commandBus`,
  `ctx.eventBus`, `ctx.notifications`, and the new `ctx.businessChat`
- never call `ctx.db.collection('business_commands')`
- never implement manual `business_commands` fallbacks
- never use package-manager imports

## Permission Model

Add Support permissions:

- `support.read`
- `support.triage`
- `support.assign`
- `support.reply`
- `support.resolve`
- `support.manage_inboxes`
- `support.manage_macros`
- `support.manage_sla`
- `support.agent_request`
- `support.agent_apply`

External send permissions must be separate from draft/apply permissions.

## Implementation Phases

### Phase 0 - Spec And Fixtures

Status: complete on 2026-06-17.

Deliverables:

- finalized this implementation plan as the working RFC
- added `src/apps/business-os/modules/support/` with module manifest, UI shell,
  command builders, reducers, schemas, locales, icon, and smoke test
- defined 21 Support collections and generated `collections.schema.json`
- added Support to `build_business_os_schema_contract.mjs` and regenerated
  `business_os_schema_contract.json`
- added Support schema hashes to `business_os_schema_hashes.json` and mirrored
  them in `rxdb/src/schema.mjs`
- rebuilt `rxdb/dist/ctox-rxdb-js.mjs` with pinned esbuild and bumped all three
  cache-busters to `20260617-support-schema`
- added Support permission constants to browser and Rust policy surfaces:
  `support.read`, `support.triage`, `support.assign`, `support.reply`,
  `support.resolve`, `support.manage_inboxes`, `support.manage_macros`,
  `support.manage_sla`, `support.agent_request`, `support.agent_apply`
- preserved the Harness model: `Ask CTOX` builds a `business_os.chat.task`
  command with a `support.agent.writeback` contract; Support does not run an
  agent or write `business_commands` manually

Validation:

- `node src/apps/business-os/modules/support/tests/support.test.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `node src/core/rxdb/tools/build_business_os_schema_contract.mjs`
- `node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/data-plane-guard-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/bundle-reproducible-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/run-all.mjs`
  - 37 passed, 0 failed, 2 skipped because the wire daemon was not built
- `cargo test native_all_schema_hashes_match_browser_contract_fixture -- --nocapture`
- `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`
- `cargo test --manifest-path src/core/rxdb/Cargo.toml`
- `node src/apps/business-os/shared/permissions.test.mjs`
- `cargo test business_os_policy_default_role_matrix_covers_major_permissions -- --nocapture`
- `cargo fmt --check`

### Phase 1 - Backend MVP

Status: complete on 2026-06-17.

Deliverables:

- added `src/core/business_os/support.rs` as the native Support command and
  projection module
- added MVP SQLite tables for conversations, thread links, identity links,
  notes, conversation events, assignment events, and Agent suggestions
- registered the Support migration from `src/core/business_os/store.rs`
- routed `support.*` commands through native `store.rs` dispatch with
  server-side policy enforcement for triage, assignment, resolve, reply, Agent
  request, and Agent apply permissions
- implemented command handlers for open, claim, assign, status, priority,
  snooze, resolve, reopen, identity link, note, ticket link/create, draft,
  Agent writeback, apply suggestion, and reject suggestion
- kept direct external send gated: `support.reply.send` currently fails unless
  a channel send gateway is configured, so Phase 5 still owns outbound send
- projected accepted Support command results back through
  `business_records` into RxDB Support collections in `rxdb_peer.rs`
- added a Support intake projector that derives Support conversations from
  inbound `communication_threads` and `communication_messages` without adding
  any HTTP data bridge
- stored structured Harness/Agent results as `support_agent_suggestions`; the
  trusted Harness/MCP actor path and workflow skill remain Phase 3 work
- added Rust coverage for core command workflows, inbound communication intake,
  and native peer command consumption/projection

Validation:

- `cargo fmt --check`
- `cargo check`
  - finished successfully with existing warnings
- `cargo test support_commands_project_core_workflow_records -- --nocapture`
- `cargo test support_intake_projects_inbound_communication_threads -- --nocapture`
- `cargo test native_peer_consumes_support_note_command_and_projects_support_records -- --nocapture`
- `cargo test support -- --nocapture`
  - 29 passed, 0 failed
- `cargo fmt --check --manifest-path src/core/rxdb/Cargo.toml`
- `cargo test --manifest-path src/core/rxdb/Cargo.toml`
  - 269 passed across unit and conformance tests, 0 failed
- `node src/apps/business-os/modules/support/tests/support.test.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `node src/apps/business-os/rxdb/tests/run-all.mjs`
  - 37 passed, 0 failed, 2 skipped because the wire daemon was not built

### Phase 2 - App MVP

Status: complete on 2026-06-17.

Deliverables:

- completed Support module file inventory with `README.md`, `module.json`,
  `collections.schema.json`, UI files, helpers, locales, icon, and tests
- registered Support in `src/apps/business-os/modules/registry.json`
- changed Support metadata to a source-bundled Store module
  (`install_scope: "store"`, `default_installed: false`) while keeping native
  Backend/RxDB ownership for Support state
- added explicit third-pane workflow justification in manifest, HTML, and CSS
- added read-only dependency collections for communication messages/threads,
  ticket cases, customer accounts, and customer contacts by importing owner
  schemas instead of duplicating them
- moved Support data access from `ctx.db.raw` to the live `ctx.db.collection`
  facade
- extended the queue with `Mine`, `Unassigned`, `All open`, `Needs reply`,
  `SLA risk`, `Snoozed`, and `Agent drafts`
- merged `communication_messages`, Business Chat messages, notes, events, and
  Agent suggestions into the Support timeline
- added right-pane controls for claim, assign-to-me, status, priority, snooze,
  resolve/reopen, and create ticket
- added read-only right-pane customer, contact, ticket, linked-thread, and
  local-message context
- fixed command builders so empty `actor: {}` no longer blocks shell
  session actor injection by `ctx.commandBus.dispatch`

Validation:

- `node --check src/apps/business-os/modules/support/index.js`
- `node src/apps/business-os/modules/support/tests/support.test.mjs`
- `node src/core/rxdb/tools/build_business_os_module_schema_files.mjs --write`
- `node src/core/rxdb/tools/build_business_os_module_schema_files.mjs`
- `runtime/build/cargo-target/debug/ctox business-os app validate support --source`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `node src/core/rxdb/tools/build_business_os_schema_contract.mjs`
- `node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/data-plane-guard-smoke.mjs`
- Browser smoke with real Business OS data plane is still pending; the
  validator/static/schema gates are green.

### Phase 3 - Harness Agent Integration

Status: complete on 2026-06-17.

Deliverables:

- added `ctx.businessChat.submitTask` to the Business OS shell context so
  modules can create normal Business Chat tasks without the legacy window-level
  event
- wired Support `Ask CTOX` to dispatch `business_os.chat.task` through that
  facade, including `required_skills`, record snapshot, response channel, and a
  `support.agent.writeback` writeback contract
- kept the command-bus fallback only for shells that do not yet expose
  `ctx.businessChat`
- used the Phase 1 native `support.agent.writeback`,
  `support.agent.apply_suggestion`, and `support.agent.reject_suggestion`
  handlers as the typed writeback surface
- added Support MCP descriptors for `support.agent.writeback`,
  `support.agent.apply_suggestion`, and `support.agent.reject_suggestion`
- changed Support MCP proposals so typed Support payloads stay unwrapped rather
  than being nested into the generic `title/objective/input` action shape
- added `skills/business-os-support-workflow/SKILL.md` with the Support Agent
  contract: no private chatbot, no external sends, write structured results via
  `support.agent.writeback`
- added a right-pane CTOX Work section with recent Support commands, related
  queue tasks, structured Agent suggestions, and apply/reject controls
- kept apply/reject as audited human disposition for now; conversion of
  suggestions into concrete draft/status/macro/customer/ticket mutations is a
  later controlled-command extension

Validation:

- `cargo fmt`
- `cargo fmt --check`
- `cargo check`
  - finished successfully with existing warnings
- `node --check src/apps/business-os/app.js`
- `node --check src/apps/business-os/modules/support/index.js`
- `node src/apps/business-os/modules/support/tests/support.test.mjs`
- `runtime/build/cargo-target/debug/ctox business-os app validate support --source`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `cargo test support_agent_action_proposal_keeps_typed_payload_unwrapped -- --nocapture`
- `cargo test support_module_actions_expose_agent_writeback_contract -- --nocapture`

### Phase 4 - Assignment, Macros, SLA

Status: complete on 2026-06-17.

Deliverables:

- added native SQLite tables and Business OS projections for:
  `support_inboxes`, `support_assignment_policies`, `support_macros`,
  `support_automation_rules`, `support_sla_policies`,
  `support_applied_slas`, and `support_sla_events`
- added typed Support commands:
  `support.inbox.upsert`, `support.assignment_policy.upsert`,
  `support.macro.upsert`, `support.macro.run`,
  `support.automation_rule.upsert`, `support.automation.evaluate`,
  `support.sla_policy.upsert`, `support.sla.apply`, and
  `support.sla.recalculate`
- extended Browser command helpers so the new command types remain allowlisted
  in the Support module test path
- changed `support.conversation.claim` to an atomic conditional SQLite update:
  unassigned, same assignee, or explicit `force` only
- added required-field gates before `support.conversation.resolve`, sourced
  from command payload, conversation custom attributes, or inbox policy
- implemented macro execution with a closed action enum:
  note creation, status, priority, assignment, snooze, ticket link,
  draft reply, and SLA apply
- implemented automation evaluation over active rules for an event name, using a
  closed condition/operator set and the same closed action enum
- implemented SLA application/recalculation with due timestamps, breach status,
  breach events, and resolve-time SLA closure
- kept all Phase 4 writes on the native command/projection path; no browser
  direct writes and no HTTP data bridge were added

Validation:

- `cargo fmt`
- `cargo fmt --check`
- `cargo check`
  - finished successfully with existing warnings
- `cargo test support -- --nocapture`
  - 34 passed, 0 failed
- `node --check src/apps/business-os/modules/support/support-commands.mjs`
- `node src/apps/business-os/modules/support/tests/support.test.mjs`
- `runtime/build/cargo-target/debug/ctox business-os app validate support --source`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`

### Phase 5 - Reply Send And Reporting

Status: complete on 2026-06-17.

Deliverables:

- changed `support.reply.send` from a hard bailout into an approval-gated
  outbound handoff:
  direct send mode is still rejected until a real channel send gateway exists,
  but normal send requests create an auditable
  `support.reply.pending_approval` conversation event
- added attachment references on reply handoff payloads using
  `desktop_files` / `desktop_file_chunks` identifiers; no HTTP file bridge or
  direct binary channel was added
- imported canonical Desktop file schemas into the Support module and declared
  `desktop_files` / `desktop_file_chunks` as read-only module dependencies
- added native SQLite tables and projections for saved views:
  `support_views` and `support_view_filters`
- added native SQLite tables and projections for reporting:
  `support_reporting_events` and `support_reporting_rollups`
- added typed Support commands:
  `support.view.upsert`, `support.view_filter.upsert`,
  `support.bulk.assign`, `support.bulk.status`, `support.bulk.priority`,
  `support.bulk.snooze`, `support.bulk.resolve`, and
  `support.reporting.rebuild_rollups`
- inserted reporting raw events for Support status changes and pending reply
  approvals
- implemented reproducible day-bucket reporting rollups from raw
  `support_reporting_events`
- added bulk action handlers that reuse the same native status, assignment,
  priority, snooze, resolve, and SLA/resolve-gate logic as single-record
  commands

Validation:

- `cargo fmt`
- `cargo fmt --check`
- `cargo check`
  - finished successfully with existing warnings
- `cargo test support -- --nocapture`
  - 36 passed, 0 failed
- `node src/apps/business-os/modules/support/tests/support.test.mjs`
- `node src/core/rxdb/tools/build_business_os_module_schema_files.mjs`
- `runtime/build/cargo-target/debug/ctox business-os app validate support --source`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `node src/core/rxdb/tools/build_business_os_schema_contract.mjs`
- `node src/apps/business-os/rxdb/tests/schema-hash-registry-smoke.mjs`
- `node src/apps/business-os/rxdb/tests/data-plane-guard-smoke.mjs`

## Live E2E Verification - 2026-06-17

Final production-readiness environment:

- Isolated CTOX root: `/tmp/ctox-support-prod-e2e.final3.GIF7Be`
- Server: `ctox business-os serve --addr 127.0.0.1:18770`
- Binary: `runtime/build/cargo-target/debug/ctox`
- Browser: headless Chromium through Playwright against
  `http://127.0.0.1:18770/#support`
- WebRTC/RxDB peer status: native peer running, `replicationUp: true`,
  `http_bridge_available: false`

Production fixes validated by this run:

- Native RxDB projection updates now use document `upsert` for
  `support_conversations`, so updates to existing parent rows converge instead
  of only changing child projections.
- Support intake ignores internal Business Chat thread keys of the form
  `business-os/support/<conversation_id>`. Those chats render in the existing
  Support timeline and no longer create duplicate Support conversations.
- `support.conversation.claim` no longer resets a conversation status back to
  `open`; it preserves an already selected `waiting` status even when commands
  complete out of order.
- The browser module applies local optimistic state before async command
  dispatch completes, so a fast `Ask CTOX` receives the current operator state
  in the task instruction and `record_snapshot`.

Final passed browser and data-plane flow:

- Seeded `conv_prod_final3` through the real CLI command dispatcher using
  `support.conversation.open_from_thread`.
- Loaded the Support module through the real Business OS shell; native peer
  reported `replicationUp: true`.
- Executed fast UI actions: claim, assign to me, priority `urgent`, status
  `waiting`, internal note, and `Ask CTOX`.
- Browser evidence after actions: queue counts `Offen=1`, `Meine=1`,
  `Unassigned=0`; visible context `status=waiting`, `priority=urgent`,
  `assignee=local-dev`; note visible in the timeline.
- Browser console/network evidence: zero console errors, zero warnings, zero
  failed requests, zero bad HTTP responses.
- Native SQLite evidence:
  `support_conversations` contained exactly one row,
  `conv_prod_final3|waiting|urgent|local-dev`; `business_commands` had no
  failed commands.
- Native RxDB SQLite evidence:
  `ctox_business_os__support_conversations__v0` held
  `conv_prod_final3|waiting|urgent|local-dev`.
- The `business_os.chat.task` payload was correct under a fast-click race:
  instruction contained `Status: waiting` and `Priority: urgent`; the
  `record_snapshot.conversation` contained `waiting`, `urgent`, `local-dev`;
  `record_snapshot.notes` length was `1`.
- A typed `support.agent.writeback` command with `client_context.actor` created
  a `draft_reply` suggestion in native SQLite and RxDB.
- Applying the suggestion from the UI produced
  `support.agent.apply_suggestion`; after reload the UI still showed
  `draft_reply · applied`, the internal note, and the `waiting/urgent` state.

Final production gates run:

- `node --check src/apps/business-os/modules/support/index.js`
- `node src/apps/business-os/modules/support/tests/support.test.mjs`
- `node src/apps/business-os/scripts/assert-module-conformance.mjs`
- `node src/apps/business-os/scripts/assert-rxdb-only.mjs`
- `node src/apps/business-os/rxdb/tests/run-all.mjs`
- `cargo test --manifest-path src/core/rxdb/Cargo.toml`
- `cargo test sync_business_record_projections_updates_existing_support_conversation_fields -- --nocapture`
- `cargo test support_intake_ignores_internal_business_chat_threads -- --nocapture`
- `cargo test support_claim_preserves_existing_waiting_status -- --nocapture`
- `cargo fmt --check`
- `cargo build --bin ctox`

## Acceptance Criteria

The Support app is ready when:

- a new inbound communication thread becomes a Support conversation
- an operator can claim, assign, prioritize, note, snooze, and resolve it
- the app can link the conversation to a Customer and Ticket
- `Ask CTOX` creates a real `ctox_queue_tasks` item
- Agent chat results appear in Business Chat
- structured Agent results appear as `support_agent_suggestions`
- applying an Agent draft is a human action and does not bypass send policy
- all Support state is visible through RxDB/WebRTC projections
- no Support feature depends on REST, ActionCable, Redis, env runtime toggles, or
  direct browser writes to `business_commands`

## Guardrails

- Do not patch `src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs`.
- Do not introduce HTTP fallback for Support records, commands, files, or Agent
  state.
- Do not copy Chatwoot's Rails/Postgres/Redis/ActionCable layers.
- Do not let Agent final prose be the only durable writeback for structured
  Support state.
- Do not let Agents send customer-facing messages.
- Do not use `window.dispatchEvent('ctox-business-os-chat-submit', ...)` in new
  Support code; add or use a shell `ctx.businessChat` API.
- Do not write `business_commands` manually from the app.
- Do not create package-manager or bundler dependencies for the module.

## Open Design Decisions

- Whether Support should be default-installed after Phase 2 or remain optional
  until reply send is complete.
- Whether `support.reply.send` should call a new generic communication send
  command or hand off to the existing Outbound approval/send machinery.
- Whether identity resolution should auto-create customer contacts or only
  create review candidates.
- Whether Agent structured writeback should primarily use MCP
  `business_os.execute_action` or CLI `ctox business-os commands dispatch`.
  Both should route through the same typed command handler.
