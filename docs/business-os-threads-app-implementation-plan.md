# Business OS Threads App Implementation Plan

## Purpose

Build a native Business OS `threads` app that gives every signed-in user a
personal, relevance-filtered hub for work that concerns them, without turning
chat or threads into the source of truth for operational work.

The app is a collaboration and attention layer over existing Business OS apps,
records, channels, tickets, queue tasks, reviews, follow-ups, and CTOX harness
state.

Apps remain the workrooms. CTOX remains the durable work orchestrator. Threads
is the user's personal command center.

## Product Principle

Business OS already has the core primitives:

- Apps own process state and app-specific workflows.
- Inbound/outbound channels carry free-form communication.
- `business_commands` and `ctox_queue_tasks` carry ad-hoc agent work.
- Tickets, support, outbound, research, documents, customers, coding agents, and
  CTOX expose durable lifecycle state.
- The shell already lets users address CTOX from app context via right click.

Threads should not duplicate those systems. It should aggregate the user's
relevant work, messages, mentions, approvals, notes, and CTOX delegations into a
single personal surface.

## User Outcomes

1. A user can open Threads and see only work relevant to them:
   assigned items, mentions, notes, approvals, delegated CTOX tasks, stale
   unresolved work, failed runs, and completed work they initiated or watch.
2. From any app element, a user can right-click and:
   - ask CTOX a read-only question
   - ask CTOX to work with the data
   - request an app change if permitted
   - leave a note for another user
   - mention another user in context
   - draft a CTOX instruction that another user must review/approve
3. Users without agent-command permissions can still capture intent in context
   and route it to an experienced user for approval.
4. Reviewers can edit, approve, reject, or redirect the requested CTOX task.
5. Approved tasks enter the normal Business OS command/queue/harness flow with
   the original app/record context and a durable approval trail.

## Non-Goals

- Do not make Threads the authoritative store for app work.
- Do not add HTTP data bridges for Business OS collections.
- Do not replace app-local timelines, ticket cases, support conversations,
  outbound approvals, research runs, or document runbooks.
- Do not make chat transcript completion a substitute for CTOX review,
  validation, queue, ticket, or app state.

## Architecture

### Existing Surfaces To Reuse

- Shell module context: `db`, `sync`, `commandBus`, `businessChat`,
  `contextMenu`, notifications, session, governance, permissions.
- Global shell right-click context menu in `src/apps/business-os/app.js`.
- `business_commands` for durable browser-to-native commands.
- `ctox_queue_tasks` for projected queue/harness state.
- `business_users` for user identity.
- Existing app records and app-local lifecycle collections.
- Existing communication collections: `communication_threads` and
  `communication_messages`.
- Existing support/outbound/ticket writeback and approval mechanics.

### New App

Add `src/apps/business-os/modules/threads/`.

The module is a normal Business OS app:

- `module.json`
- `schema.js`
- `index.html`
- `index.js`
- `index.css`
- `locales/de.json`
- `locales/en.json`
- tests

Layout:

- left pane: personal filters, channels, teams, watches
- center pane: thread list and focused thread timeline
- right pane: linked app context, approvals, CTOX backing state, participants

## Data Model

Use RxDB/WebRTC collections only. Native state is projected through the normal
Business OS store/peer path.

### `user_threads`

Personal and shared collaboration threads.

Fields:

- `id`
- `thread_key`
- `title`
- `kind`: `note`, `mention`, `approval_request`, `ctox_task`, `app_event`,
  `channel_thread`, `system`
- `scope`: `personal`, `team`, `app_record`, `channel`, `private`
- `source_module`
- `source_record_type`
- `source_record_id`
- `source_label`
- `source_deep_link`
- `owner_user_id`
- `created_by_user_id`
- `assignee_user_ids`
- `participant_user_ids`
- `watcher_user_ids`
- `status`: `open`, `waiting`, `needs_review`, `approved`, `rejected`,
  `running`, `blocked`, `completed`, `archived`
- `priority`
- `last_activity_at_ms`
- `last_seen_by_user`
- `payload`
- `created_at_ms`
- `updated_at_ms`
- `is_deleted`

### `user_thread_messages`

Internal messages, notes, and system events inside a thread.

Fields:

- `id`
- `thread_id`
- `message_type`: `note`, `mention`, `approval_request`, `approval_decision`,
  `ctox_status`, `system`, `app_event`
- `author_user_id`
- `body`
- `mentions_user_ids`
- `attachments`
- `source_command_id`
- `source_task_id`
- `source_message_key`
- `source_app_record`
- `payload`
- `created_at_ms`
- `updated_at_ms`
- `is_deleted`

### `user_thread_links`

Typed links to canonical work.

Fields:

- `id`
- `thread_id`
- `link_type`: `app_record`, `business_command`, `queue_task`, `ticket_case`,
  `ticket_self_work`, `support_conversation`, `outbound_engagement`,
  `communication_thread`, `communication_message`, `research_run`,
  `document`, `coding_agent_session`
- `module`
- `record_type`
- `record_id`
- `thread_key`
- `message_key`
- `task_id`
- `command_id`
- `case_id`
- `deep_link`
- `label`
- `payload`
- `created_at_ms`
- `updated_at_ms`

### `user_notifications`

Per-user notification/read-state projection.

Fields:

- `id`
- `user_id`
- `thread_id`
- `reason`: `mentioned`, `assigned`, `approval_requested`, `ctox_completed`,
  `ctox_failed`, `waiting_on_user`, `stale_unresolved`, `watch_update`
- `status`: `unread`, `read`, `dismissed`, `snoozed`
- `snoozed_until_ms`
- `created_at_ms`
- `updated_at_ms`

### `ctox_task_approval_requests`

Approval gate for user-authored CTOX prompts.

Fields:

- `id`
- `thread_id`
- `requested_by_user_id`
- `reviewer_user_id`
- `source_module`
- `source_record_type`
- `source_record_id`
- `source_label`
- `source_context`
- `requested_command_type`
- `requested_payload`
- `requested_client_context`
- `prompt`
- `required_skills`
- `writeback_contract`
- `status`: `draft`, `pending_review`, `approved`, `rejected`, `cancelled`,
  `submitted`, `failed`
- `decision_by_user_id`
- `decision_note`
- `approved_command_id`
- `approved_task_id`
- `created_at_ms`
- `updated_at_ms`

## Command Types

Add typed native command handlers instead of direct browser mutation for
privileged workflow transitions.

### Notes And Mentions

- `threads.note.create`
- `threads.note.update`
- `threads.note.delete`
- `threads.message.create`
- `threads.thread.watch`
- `threads.thread.unwatch`
- `threads.thread.archive`
- `threads.thread.snooze`

### Approval Requests

- `threads.ctox_approval.request`
- `threads.ctox_approval.edit`
- `threads.ctox_approval.approve`
- `threads.ctox_approval.reject`
- `threads.ctox_approval.cancel`

### Projection/Linking

- `threads.link.create`
- `threads.link.remove`
- `threads.notification.mark_read`
- `threads.notification.dismiss`

Approval should be server-authoritative. The browser can stage drafts, but
approval and command submission must be handled by native policy and store code.

## Permission Model

Add or reuse policy concepts so behavior is explicit:

- Any authenticated user can create personal notes and mention users within
  records they can view.
- Users can ask CTOX read-only questions where `data.read` is allowed.
- Users without `ctox.task.create` or equivalent app permission can create
  `threads.ctox_approval.request`, not direct agent tasks.
- Reviewers need a permission such as `threads.ctox_approval.review` or an
  existing role/scope that permits CTOX task creation for that app/record.
- Approval must re-check the original user, reviewer, app, record, and command
  scope at decision time.
- Approved CTOX commands must include:
  - original requester
  - reviewer
  - approval request id
  - source module/record context
  - visible scope from right-click context

## Shell Integration

Extend the global right-click menu so it supports human collaboration in
addition to CTOX actions.

Current modes:

- work with data
- answer question
- modify app

Add:

- leave note
- mention user
- request CTOX approval

### Right-Click Note Flow

1. User right-clicks any app element.
2. Shell extracts module, column, record type/id, selected text, clicked text,
   and visible scope.
3. User selects `Note`.
4. User enters body and optional recipients.
5. Browser dispatches `threads.note.create`.
6. Native handler persists thread/message/link/notification projections.
7. Recipient sees the item in Threads and can deep-link back into the source app.

### Right-Click Approval Flow

1. User right-clicks app element.
2. User chooses `Request CTOX approval`.
3. User writes the intended CTOX prompt.
4. User tags reviewer.
5. Browser dispatches `threads.ctox_approval.request`.
6. Threads app shows pending approval to reviewer.
7. Reviewer can edit, approve, reject, or ask for clarification.
8. On approval, native code creates the intended `business_commands` document or
   calls the existing command admission path.
9. The resulting command/task ids are linked back to the thread.

## Threads App UX

### Left Pane

Filters:

- Inbox
- Mentions
- Waiting on me
- CTOX approvals
- Delegated by me
- Running CTOX work
- Failed/blocking
- Watching
- Snoozed
- Archived

Optional grouping:

- by app
- by customer/project
- by priority
- by due date
- by source channel

### Center Pane

Thread list with:

- source app icon/name
- title
- participants
- reason for relevance
- status
- last activity
- CTOX task status if linked
- unread marker

Focused timeline:

- internal notes
- mentions
- approval requests/decisions
- app events
- CTOX status events
- linked inbound/outbound messages

### Right Pane

Context:

- source app/record
- deep link
- participants/watchers
- linked command/task/ticket/support/outbound/research/document state
- approval controls
- CTOX status/evidence summary

## Relevance Projection

Implement a native projection pass that derives per-user thread relevance from:

- `user_thread_messages.mentions_user_ids`
- assignee/reviewer fields
- `ctox_task_approval_requests.reviewer_user_id`
- tasks delegated by the user
- queue tasks that completed/failed after last seen
- app records watched by the user
- support/ticket/outbound approvals waiting on the user
- stale unresolved work assigned to or watched by the user

This should produce/update `user_notifications` and thread status summaries.

## App Linking Contract

Every app should be able to create thread links with the same shape:

- `source_module`
- `source_record_type`
- `source_record_id`
- `source_label`
- `deep_link`
- `thread_key`
- optional `command_id`
- optional `task_id`
- optional `case_id`
- optional `message_key`

The global right-click extractor can infer many links from DOM attributes, but
apps should annotate important records explicitly with `data-context-*` where
precision matters.

## CTOX Approval Semantics

Approval is not just a UI button. It is a durable work gate.

Rules:

- A pending approval request is not a CTOX task.
- A rejected approval request must not create a queue task.
- A reviewer may edit the prompt before approval.
- The final approved prompt is the submitted prompt of record.
- Approval must create audit evidence linking requester, reviewer, prompt,
  source context, command id, and task id.
- If the reviewer lacks permission at approval time, the request remains blocked.
- If the source record/app is no longer available, the reviewer must choose
  re-target, reject, or submit with stale-context warning.

## Native Implementation Steps

1. Add schemas to `src/apps/business-os/modules/threads/schema.js`.
2. Add generated/wire contracts as required by the RxDB contract workflow.
3. Add native store tables for thread, message, link, notification, and approval
   request records.
4. Add projection functions into the Business OS native peer loop.
5. Add command handlers in `src/core/business_os/store.rs` or a dedicated
   `threads.rs` module.
6. Add permission decisions to `src/core/business_os/policy.rs`.
7. Add browser module under `src/apps/business-os/modules/threads/`.
8. Register module in the module catalog/registry.
9. Extend global shell right-click menu with note/mention/approval modes.
10. Add tests for:
    - note create from app context
    - mention notification
    - approval request without CTOX task permission
    - approval creates a real command/task
    - rejection creates no queue task
    - permission revocation blocks approval
    - source context/deep link preserved

## Migration Strategy

Phase 1: Minimal MVP

- Threads app with Inbox/Mentions/Approval filters.
- Right-click `Leave note`.
- Right-click `Request CTOX approval`.
- Native approval command creates normal `business_os.chat.task` on approval.

Phase 2: App Backing State

- Link thread cards to `business_commands`, `ctox_queue_tasks`, ticket cases,
  support conversations, outbound approvals, and research/document runs.
- Add per-user notification/read state.
- Add watchers/snooze/archive.

Phase 3: Rich Collaboration

- User-to-user direct notes.
- Team threads.
- App-record comment timelines.
- Better participant/watch controls.
- Inline reviewer prompt editing.

Phase 4: Proactive Attention

- Stale unresolved work projection.
- Waiting-on-me detection.
- Completed/failed since last seen.
- Suggested reviewer routing based on app ownership/roles.

## Acceptance Criteria

- A user can right-click a ticket, support conversation, outbound campaign,
  research task, document, customer record, or CTOX task and leave a note for
  another user.
- The recipient sees that note in Threads with a deep link back to the source
  app/record.
- A low-permission user can draft a CTOX request from app context and route it
  to an experienced reviewer.
- No queue task is created until a reviewer approves.
- On approval, the resulting CTOX command/task is visible both in Threads and in
  the relevant app/CTOX surfaces.
- The original requester, reviewer, source app, source record, prompt, decision,
  command id, and task id are auditable.
- All data moves through Business OS RxDB/WebRTC and native command handling,
  not through a new HTTP data bridge.

## Risks

- If Threads becomes a second source of truth, app lifecycle integrity erodes.
- If right-click actions bypass native policy, low-permission users can escalate.
- If approval creates direct queue rows instead of normal command admission,
  command status and projections drift.
- If notifications are purely browser-local, multi-user state becomes unreliable.
- If every app invents its own context shape, the hub becomes inconsistent.

## Design Rule

Threads should make Business OS feel more like a shared, persistent agentic team
environment without copying Slack or Discord as the underlying work model.

The app is for attention, collaboration, and approval. The work remains in the
apps and in CTOX durable runtime state.
