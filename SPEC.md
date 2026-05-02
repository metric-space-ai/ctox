# CTOX Ticket Integration Specification

Status: Draft v1

Purpose: Define how CTOX should use its built-in ticket subsystem to interact
reliably with synchronized external ticket systems while preserving CTOX itself
as the runtime state machine, queue, and orchestration authority. This
specification adapts useful Symphony principles such as normalization,
preflight, reconciliation, retry, observability, and test matrices to CTOX's
internal architecture. Other ticket systems are sync sources and targets behind
adapters; they do not lead CTOX execution. Sync MAY be bidirectional when an
adapter supports the required capabilities.

## Normative Language

The key words `MUST`, `MUST NOT`, `REQUIRED`, `SHOULD`, `SHOULD NOT`,
`RECOMMENDED`, `MAY`, and `OPTIONAL` in this document are to be interpreted as
described in RFC 2119.

`Implementation-defined` means the behavior is part of the CTOX implementation
contract but may vary by adapter, deployment, or runtime policy. Implementations
MUST document implementation-defined behavior when it affects dispatch,
approval, writeback, or ticket closure.

## 1. Problem Statement

CTOX is a long-running agentic daemon for technical work that spans tickets,
messages, schedules, approvals, queue items, verification, and durable context.
CTOX itself is the state machine, including the mission queue and runtime
governance. CTOX also has a built-in ticket subsystem. That subsystem is not a
thin adapter around an external issue tracker and it is not a replacement for
the CTOX queue. It is the durable ticket-integration layer inside CTOX: it
mirrors synchronized ticket data, stores source knowledge, records cases,
approvals, verifications, writebacks, self-work, and audit evidence, and gives
the CTOX state machine reliable ticket semantics.

The system solves five operational problems:

- It converts ticket events into durable, auditable CTOX work instead of
  relying on transient agent memory.
- It gates work through source knowledge, label/control bundles, dry runs,
  approvals, autonomy grants, verification, and writeback policy.
- It connects ticket events and self-work to CTOX's mission queue so the daemon
  can arbitrate work alongside chat, email, plan, schedule, and follow-up
  producers.
- It uses a core state machine and transition guard for behavior that cannot be
  left to prompt discipline.
- It records process evidence so recovery, review, and learning can inspect how
  a ticket moved through the system.

Important boundary:

- CTOX MUST NOT be specified as a Linear-only issue tracker poller.
- CTOX itself, including its queue and core state machine, remains the
  orchestration authority.
- CTOX's built-in ticket subsystem MUST be specified as the ticket-integration
  and ticket-control layer that helps CTOX interact with synchronized ticket
  systems.
- CTOX MAY sync with any ticket system that implements the adapter contract.
  Current adapters such as `local` and `zammad` are examples, not architectural
  limits.
- The durable source of truth for CTOX runtime and ticket integration state is
  `runtime/ctox.sqlite3`.
- Agent runs SHOULD use the `ctox ticket`, `ctox queue`, `ctox verification`,
  `ctox process-mining`, and related command surfaces to mutate durable state.

## 2. Goals and Non-Goals

### 2.1 Goals

- Normalize adapter-sourced data into CTOX's built-in ticket subsystem.
- Route inbound ticket events through explicit routing state with leasing and
  acknowledgement.
- Require source onboarding and ticket knowledge before active handling.
- Resolve executable synchronized ticket events through a label assignment,
  control bundle, autonomy grant, dry run, and case record before they become
  CTOX queue work.
- Dispatch ticket work through CTOX's queue and worker loop with working-hours,
  runtime-blocker, and queue-pressure guards.
- Represent internal CTOX follow-up as first-class self-work that can be
  published or synchronized to any adapter-backed ticket system when supported.
- Use a core state machine for ticket, work item, queue, review, founder
  communication, schedule, commitment, knowledge, and repair transitions.
- Require verification evidence before owner-visible ticket or self-work
  closure.
- Record audit and harness-flow events that allow operators and agents to
  reconstruct the ticket lifecycle.
- Support learning candidates that can promote safer future autonomy only after
  explicit decision.

### 2.2 Non-Goals

- Linear-specific state, GraphQL API, project slug, or workflow file semantics.
- Treating any external tracker state as the only execution state.
- Treating CTOX's ticket subsystem as a replacement for the CTOX queue or core
  state machine.
- A general-purpose distributed job scheduler.
- Bypassing CTOX's runtime SQLite store with ad hoc files for ticket, queue, or
  work-state ownership.
- Allowing an agent to close tickets based only on final prose.
- Requiring every ticket adapter to support every writeback capability.
- Mandating a web dashboard. CLI, TUI, logs, and runtime queries are sufficient
  operator surfaces.

## 3. System Overview

### 3.1 Main Components

1. `Ticket Sync Adapter`
   - Synchronizes records between the built-in CTOX ticket system and an
     adapter-backed ticket system.
   - Fetches remote tickets and events when the adapter is source-facing.
   - Publishes optional CTOX self-work items when the adapter is sink-facing.
   - Performs comment, transition, assignment, note, and self-work writebacks
     when supported by that adapter.
   - Declares capabilities through `TicketAdapterCapabilities`.

2. `Ticket Translation Layer`
   - Accepts `TicketSyncBatch` records from adapters.
   - Upserts normalized `ticket_items` and `ticket_events`.
   - Creates routing rows and records `ticket_sync_runs`.
   - Ensures source-control adoption rows exist.

3. `Ticket Control Plane`
   - Owns labels, control bundles, autonomy grants, cases, dry runs, approvals,
     execution actions, verifications, writebacks, learning candidates, and
     audit records.
   - Resolves whether work is dry-run-only, approval-gated, bounded-auto, or
     directly executable.

4. `Ticket Knowledge Plane`
   - Stores source-specific operational knowledge in `ticket_knowledge_entries`.
   - Requires knowledge loads before dry runs.
   - Blocks ticket event routing when required domains are missing.
   - Links source skills and runtime-generated skillbooks/runbooks to ticket
     sources.

5. `Mission Queue`
   - Stores queue tasks as inbound messages in `communication_messages` with
     `channel = 'queue'`.
   - Tracks queue state in `communication_routing_state`.
   - Provides priorities, thread keys, workspace roots, suggested skills,
     parent links, and queue/self-work bridge metadata.

6. `Service Router`
   - Runs periodic routing when the active worker loop is idle.
   - Syncs configured ticket systems.
   - Leases inbound communication and ticket events.
   - Blocks unsafe or incomplete work before it reaches the active loop.
   - Enqueues prepared prompts into the in-memory pending prompt queue or starts
     a worker immediately.

7. `Agent Worker`
   - Runs a bounded turn through CTOX's in-process harness and model gateway.
   - Emits events, synchronizes mission state, performs completion review, and
     updates queue, ticket event, self-work, and communication routing state.

8. `Core Transition Guard`
   - Validates protected state transitions using `CoreTransitionRequest`.
   - Enforces review, verification, founder communication, commitment,
     schedule, repair, and knowledge evidence requirements.

9. `Harness Flow Evidence`
   - Records chainable events in `ctox_harness_flow_events`.
   - Links `message_key`, `work_id`, `ticket_key`, attempt index, and metadata.

10. `Governance`
    - Records runtime guard decisions for blocked routing, queue pressure,
      runtime backoff, ticket knowledge gates, and continuation mechanisms.

### 3.2 Layering

CTOX implementations SHOULD preserve these boundaries:

1. `Adapter Layer`
   - Native Rust ticket adapters. Current examples include `local` and
     `zammad`.
   - Adapter-specific auth, pagination, metadata enrichment, publication, and
     remote writes.

2. `Normalization Layer`
   - Stable ticket and event records independent of adapter shape.
   - Canonical ticket and event keys.

3. `Control Layer`
   - Knowledge gates, label assignment, bundles, autonomy grants, dry runs,
     approvals, cases, and verification.

4. `Queue Layer`
   - Durable queue tasks and routing state in the communication tables.
   - In-memory ordered prompt queue for active service arbitration.

5. `Execution Layer`
   - Turn-loop invocation, context assembly, completion review, continuity
     refresh, and result classification.

6. `Governance Layer`
   - State-machine validation, durable audits, process events, queue repair,
     and runtime guard evidence.

### 3.3 Symphony Principles Adapted for CTOX

The Symphony service specification is useful as an operations reference, not as
a leadership model for CTOX. CTOX SHOULD adapt these principles:

- Normalize external ticket payloads before routing them.
- Keep a clear adapter boundary between ticket-system APIs and runtime
  execution.
- Run dispatch preflight before starting work from synchronized sources.
- Reconcile stale active work, leases, and source state before dispatching new
  work.
- Preserve enough identifiers to reconnect a worker slice to the source ticket,
  queue message, workspace, and runtime evidence.
- Use explicit retry, timeout, blocked, and terminal states instead of relying
  on final prose.
- Expose operator-visible audit, status, and failure information.
- Maintain a validation/test matrix for sync, dispatch, retry, reconciliation,
  and writeback.

CTOX MUST NOT adopt these Symphony assumptions:

- An external tracker is the orchestration source of truth.
- Linear-specific schema or state names are normative.
- A repo `WORKFLOW.md` controls CTOX runtime behavior.
- A per-ticket external Codex subprocess is the required execution model.
- Queue and state-machine behavior is secondary to tracker state.

## 4. Persistence Model

### 4.1 Runtime Store

The authoritative runtime store is:

```text
runtime/ctox.sqlite3
```

All ticket, queue, communication, plan, schedule, governance, continuity,
verification, and harness-flow state SHOULD live in this store unless a
subsystem explicitly owns a separate tool-local store.

SQLite behavior:

- The runtime store SHOULD use WAL mode.
- Connections SHOULD set the configured SQLite busy timeout.
- State-changing APIs MUST be idempotent where duplicate sync, repeated
  routing, or retry paths can occur.

### 4.2 Core Tables

Ticket mirror and event routing:

- `ticket_items`
- `ticket_events`
- `ticket_event_routing_state`
- `ticket_outbound_event_marks`
- `ticket_sync_runs`

Source onboarding and knowledge:

- `ticket_source_controls`
- `ticket_source_skill_bindings`
- `ticket_knowledge_entries`
- `ticket_knowledge_loads`
- `knowledge_main_skills`
- `knowledge_skillbooks`
- `knowledge_runbooks`
- `knowledge_runbook_items`
- `knowledge_embeddings`

Self-work:

- `ticket_self_work_items`
- `ticket_self_work_assignments`
- `ticket_self_work_notes`

Control execution:

- `ticket_label_assignments`
- `ticket_control_bundles`
- `ticket_autonomy_grants`
- `ticket_cases`
- `ticket_dry_runs`
- `ticket_approvals`
- `ticket_execution_actions`
- `ticket_verifications`
- `ticket_learning_candidates`
- `ticket_writebacks`
- `ticket_audit_log`

Queue and communication:

- `communication_accounts`
- `communication_threads`
- `communication_messages`
- `communication_routing_state`
- `communication_sync_runs`
- `communication_founder_reply_reviews`
- `queue_ticket_spills`

Process evidence:

- `ctox_harness_flow_events`
- governance event tables as defined by the governance subsystem
- process-mining tables as defined by the process-mining subsystem

### 4.3 Dispatcher Runtime State

CTOX has two runtime-state layers:

- Durable runtime state in `runtime/ctox.sqlite3`.
- Ephemeral in-process arbitration state owned by the running service.

The CTOX runtime state machine owns business truth. The ephemeral layer MAY
decide what to run next, but it MUST NOT become the only record of ticket
integration, queue state, approval, verification, or open follow-up.

The service runtime state SHOULD include:

- `busy`: whether an agent worker is currently executing.
- `pending_prompts`: ordered in-memory `QueuedPrompt` entries waiting for a
  worker slot.
- `current_goal_preview`: short operator-visible summary of the active prompt.
- `active_source_label`: source currently occupying the worker, such as
  `queue`, `ticket:<system>`, `email:owner`, or `plan`.
- `last_completed_at`: timestamp of the last finished worker slice.
- `last_progress_epoch_secs`: monotonic progress marker for idle/watchdog
  checks.
- `last_error`: compact runtime error summary.
- `last_reply_chars`: size of the latest successful or synthetic failure reply.
- `leased_message_keys`: queue/communication message keys held by the active
  worker.
- `leased_ticket_event_keys`: ticket event keys held by the active worker.
- `runtime_blocker_backoff`: current hard-runtime-blocker cooldown, if any.
- `queue_pressure_guard`: whether queue pressure repair is active.
- working-hours snapshot and hold reason.

The durable layer MUST include enough state to reconstruct useful CTOX work
after a service restart:

- queue messages and route status in `communication_messages` and
  `communication_routing_state`
- ticket events and route status in `ticket_events` and
  `ticket_event_routing_state`
- ticket cases, dry runs, approvals, execution actions, verifications, and
  writebacks used to control synchronized ticket interaction
- self-work items, assignments, and notes
- audit, governance, and harness-flow evidence

The queue remains part of CTOX's state machine. The ticket subsystem MUST NOT
take over queue ownership. Instead, it supplies ticket-derived facts, cases,
constraints, verification requirements, and writeback targets so the CTOX queue
can schedule and execute synchronized ticket work safely.

## 5. Built-In Ticket Subsystem and Sync Adapter Contract

### 5.1 Built-In Ticket Subsystem

CTOX's built-in ticket subsystem is the canonical ticket-integration layer. It
stores:

- normalized tickets and events
- source adoption state
- source skills and knowledge
- labels, bundles, grants, dry runs, approvals, cases, verification, and
  writebacks
- self-work and remote publication metadata
- audit and flow evidence

Adapter-backed systems MUST NOT own CTOX execution state. They MAY provide
input events, receive published self-work, mirror state changes, and receive
writebacks through adapters.

### 5.2 Adapter Extensibility

Any ticket system can be synchronized with CTOX by implementing the adapter
contract. Current adapter examples:

- `local`
- `zammad`

These examples are not a closed list. An implementation MAY add adapters for
project trackers, help desks, support desks, CRM cases, custom databases, or
proprietary systems, provided the adapter exposes the same normalized contract
and capability declaration.

Each adapter MUST have a stable `system` name. The `system` value scopes remote
IDs, tickets, events, source controls, knowledge, and self-work publication
metadata.

### 5.3 Capabilities

Adapters MUST declare:

- `can_sync`
- `can_test`
- `can_comment_writeback`
- `can_transition_writeback`
- `can_create_self_work_items`
- `can_assign_self_work_items`
- `can_append_self_work_notes`
- `can_transition_self_work_items`
- `can_internal_comments`
- `can_public_comments`
- `state_transition_by_name`

The control plane MUST check capabilities before attempting remote writes.
Unsupported writes MUST fail before mutating the remote system.

### 5.4 Sync Batch

Adapters return a `TicketSyncBatch`:

- `system`
- `fetched_ticket_count`
- `tickets`
- `events`
- `metadata`

The sync batch is the boundary between arbitrary ticket systems and the built-in
CTOX ticket ledger. Adapter-specific fields MUST be retained in `metadata` when
they are needed for audit, writeback, source skill behavior, or support
diagnostics.

Each normalized ticket contains:

- `remote_ticket_id`
- `title`
- `body_text`
- `remote_status`
- `priority`
- `requester`
- `metadata`
- `external_created_at`
- `external_updated_at`

Each normalized event contains:

- `remote_ticket_id`
- `remote_event_id`
- `direction`
- `event_type`
- `summary`
- `body_text`
- `metadata`
- `external_created_at`

### 5.5 Ticket and Event Keys

CTOX MUST derive stable local keys from source system plus remote ID:

- `ticket_key`: canonical key for the mirrored ticket.
- `event_key`: canonical key for the mirrored event.

Implementations SHOULD use these local keys for routing, audit, and prompt
rendering. Remote IDs SHOULD be used only for adapter calls.

### 5.6 Config

Adapter runtime settings MUST come from CTOX runtime configuration and secret
state, not from ad hoc process environment reads inside execution logic.

Adapter settings are implementation-defined by adapter kind. Current Zammad
settings include:

- `CTO_ZAMMAD_BASE_URL`
- `CTO_ZAMMAD_TOKEN`
- `CTO_ZAMMAD_USER`
- `CTO_ZAMMAD_PASSWORD`
- `CTO_ZAMMAD_HTTP_TIMEOUT_SECS`
- `CTO_ZAMMAD_PAGE_SIZE`
- `CTO_ZAMMAD_ARTICLE_TYPE`
- `CTO_ZAMMAD_COMMENT_INTERNAL`
- `CTO_ZAMMAD_SELF_WORK_GROUP`
- `CTO_ZAMMAD_SELF_WORK_CUSTOMER`
- `CTO_ZAMMAD_SELF_WORK_PRIORITY`

Configured ticket systems are read from `CTOX_TICKET_SYSTEMS` by the service
router.

## 6. Source Adoption and Knowledge

### 6.1 Source Control

On first sync for a source, CTOX MUST create a `ticket_source_controls` row if
none exists.

Default adoption mode:

```text
baseline_observe_only
```

The baseline cutoff MUST be derived from the latest observed ticket/event time
in the sync batch. This allows safe onboarding without immediately treating old
history as runnable work.

### 6.2 Required Knowledge Domains

Before active ticket handling, CTOX requires source knowledge domains:

- `source_profile`
- `label_catalog`
- `glossary`
- `service_catalog`
- `infrastructure_assets`
- `team_model`
- `access_model`
- `monitoring_landscape`

`ctox ticket knowledge-load --ticket-key <key>` MUST record loaded entries and
gap domains in `ticket_knowledge_loads`.

If any required domains are missing, dry-run creation MUST fail with a ticket
knowledge gate error. The service router MUST acknowledge the leased event as
`blocked` and record governance evidence with mechanism
`ticket_knowledge_gate`.

### 6.3 Source Skills

Ticket source skills bind source systems to operational knowledge:

- `ctox ticket source-skill-set`
- `ctox ticket source-skill-show`
- `ctox ticket source-skill-query`
- `ctox ticket source-skill-import-bundle`
- `ctox ticket source-skill-resolve`

The preferred skill for a live ticket source SHOULD come from an active
`ticket_source_skill_bindings` row. Self-work metadata MAY override the skill.

## 7. Ticket Control Plane

### 7.1 Label Assignment

Every executable ticket event from any adapter source MUST resolve to one of:

- A durable `ticket_label_assignments` row.
- A synthetic self-work label when the mirrored ticket corresponds to
  `ticket_self_work_items`.

Labels select control bundles. Unlabeled adapter-sourced tickets MUST NOT enter
active execution unless a valid synthetic self-work route exists.

### 7.2 Control Bundle

A `ticket_control_bundles` row defines the operational contract for a label:

- `label`
- `bundle_version`
- `runbook_id`
- `runbook_version`
- `policy_id`
- `policy_version`
- `approval_mode`
- `autonomy_level`
- `verification_profile_id`
- `writeback_profile_id`
- `support_mode`
- `default_risk_level`
- `execution_actions_json`
- `notes`

Default execution actions:

- `observe`
- `analyze`
- `draft_communication`

Additional action classes MAY include:

- `local_safe_change`
- `repo_change`
- `remote_write`
- `privileged_change`
- `service_affecting_change`

### 7.3 Approval Modes

Canonical approval modes:

- `dry_run_only`
- `human_approval_required`
- `bounded_auto_execute`
- `direct_execute_allowed`

Initial case state:

- `dry_run_only` -> `blocked`
- `human_approval_required` -> `approval_pending`
- `bounded_auto_execute` -> `executable`
- `direct_execute_allowed` -> `executable`

### 7.4 Autonomy Levels

Canonical autonomy levels:

- `A0`
- `A1`
- `A2`
- `A3`
- `A4`

Autonomy grants MAY relax bundle defaults only through
`ticket_autonomy_grants`. Effective control MUST choose the more restrictive
approval mode and autonomy level between the bundle and grant when no valid
grant applies.

### 7.5 Dry Run

Before active handling, CTOX MUST create a dry run:

```text
ctox ticket dry-run --ticket-key <key> [--understanding <text>] [--risk-level <level>]
```

Dry-run creation MUST:

1. Load the mirrored ticket.
2. Create a ticket knowledge load.
3. Fail if required knowledge domains are missing.
4. Resolve label, bundle, and effective control.
5. Create a `ticket_cases` row.
6. Create a `ticket_dry_runs` row.
7. Record audit entries for contract resolution and dry-run artifact.

The dry-run artifact MUST include:

- ticket understanding
- ticket key
- knowledge load status, domains, gaps, and entries
- bound label
- runbook and policy identity
- effective approval mode and autonomy level
- support mode
- requested control
- autonomy grant metadata, if any
- planned actions
- actions executed during dry run
- actions simulated only
- missing approvals
- required evidence

## 8. Ticket Case State Machine

### 8.1 Case States

CTOX ticket cases use implementation states stored in `ticket_cases.state`.
Canonical states include:

- `blocked`
- `approval_pending`
- `executable`
- `executing`
- `writeback_pending`
- `closed`

The core transition bridge maps implementation states to `CoreState`:

- `created`, `open`, `queued` -> `Created`
- `classified` -> `Classified`
- `planned`, `ready` -> `Planned`
- `executing`, `in_progress`, `running` -> `Executing`
- `awaiting_review`, `review`, `reviewing` -> `AwaitingReview`
- `rework_required`, `rework` -> `ReworkRequired`
- `awaiting_verification`, `verification` -> `AwaitingVerification`
- `verified`, `writeback_pending` -> `Verified`
- `closed`, `done`, `completed` -> `Closed`
- `blocked` -> `Blocked`

Unknown case states MUST NOT be passed to the core transition guard.

### 8.2 Approval

Approval command:

```text
ctox ticket approve --case-id <id> --status <approved|rejected>
```

Effects:

- Inserts `ticket_approvals`.
- `approved` transitions the case to `executable`.
- `rejected` transitions the case to `blocked`.
- Records an audit event with actor type `approver`.

### 8.3 Execution

Execution command:

```text
ctox ticket execute --case-id <id> --summary <text>
```

Precondition:

- Case state MUST be `executable` or `executing`.

Effects:

- Inserts `ticket_execution_actions`.
- Sets case state to `executing`.
- Records an audit event with actor type `agent`.

### 8.4 Verification

Verification command:

```text
ctox ticket verify --case-id <id> --status <passed|failed> [--summary <text>]
```

Effects:

- Inserts `ticket_verifications`.
- `passed` transitions the case to `writeback_pending`.
- `failed` transitions the case to `blocked`.
- Records an audit event with actor type `verification_engine`.

### 8.5 Writeback

Comment writeback:

```text
ctox ticket writeback-comment --case-id <id> --body <text> [--internal]
```

Transition writeback:

```text
ctox ticket writeback-transition --case-id <id> --state <value> [--body <text>] [--internal]
```

Preconditions:

- Case state MUST be `writeback_pending` or `verifying`.
- Adapter MUST support the requested operation and comment visibility.

Transition writeback MUST enforce ticket closure through the core transition
guard before remote transition.

Effects:

- Calls the adapter.
- Syncs the source after remote write.
- Marks remote outbound event IDs.
- Inserts `ticket_writebacks`.
- Records audit.
- Transition writeback sets the case to `closed`.

### 8.6 Closure

Close command:

```text
ctox ticket close --case-id <id> [--summary <text>]
```

Closure MUST pass `enforce_ticket_case_close_transition`.

The core transition guard requires:

- Entity type `Ticket`.
- Lane `P2MissionDelivery`.
- Event `Close`.
- Target state `Closed`.
- Latest passed ticket verification ID.
- Review audit evidence for owner-visible completion.

A case MUST NOT close solely because an agent reply says work is done.

## 9. Ticket Event Routing

### 9.1 Event Routing States

Canonical ticket event route statuses:

- `pending`
- `leased`
- `observed`
- `handled`
- `failed`
- `duplicate`
- `blocked`

Inbound events are routable when:

- `direction = 'inbound'`
- route status is `pending` or `leased`
- lease owner is empty or matches the current lease owner

The service uses lease owner:

```text
ctox-channel-router
```

### 9.2 Lease

Lease command:

```text
ctox ticket take [--lease-owner <owner>] [--limit <n>]
```

Lease behavior:

- Selects inbound events by oldest external creation time.
- Marks selected rows `leased`.
- Sets `lease_owner`, `leased_at`, and `updated_at`.
- Clears no acknowledgement until ack.

### 9.3 Prepare for Prompt

Preparing a ticket event MUST:

1. Load the event and ticket.
2. Resolve ticket control.
3. Create a dry run.
4. Load the case created by the dry run.
5. Derive a ticket thread key.
6. Return a `RoutedTicketEvent` with label, bundle, case, dry-run artifact,
   support mode, approval mode, autonomy level, risk level, and prompt data.

If preparation fails due to missing knowledge or control state, the service
MUST acknowledge the event as `blocked` and record governance evidence.

### 9.4 Ack

Ack command:

```text
ctox ticket ack --status <handled|failed|duplicate|blocked> <event-key>...
```

Ack behavior:

- `handled`, `duplicate`, and `blocked` set `acked_at`.
- `failed` clears lease fields but does not set `acked_at`.
- All acknowledgements clear `lease_owner` and `leased_at`.

## 10. Mission Queue

### 10.1 Queue Tasks

CTOX queue tasks are communication messages:

- `channel = 'queue'`
- `account_key = 'queue:system'`
- `direction = 'inbound'`
- `folder_hint = 'queue'`
- `trust_level = 'high'`

Queue task fields:

- `message_key`
- `thread_key`
- `title`
- `prompt`
- `workspace_root`
- `ticket_self_work_id`
- `priority`
- `suggested_skill`
- `parent_message_key`
- `route_status`
- `status_note`
- `lease_owner`
- `leased_at`
- `acked_at`
- `created_at`
- `sort_at`
- `updated_at`

### 10.2 Queue Creation

Queue command:

```text
ctox queue add --title <label> --prompt <text> [--thread-key <key>] [--workspace-root <path>] [--skill <name>] [--priority <urgent|high|normal|low>] [--parent-message-key <key>]
```

Queue creation MUST:

- Validate title and prompt are non-empty.
- Canonicalize priority.
- Generate a stable queue message key.
- Store queue metadata as JSON.
- Upsert a queue communication message.
- Refresh the thread.
- Ensure inbound routing rows exist.

### 10.3 Queue Priority

Canonical priorities:

- `urgent`
- `high`
- `normal`
- `low`

`sort_at` priority shifts:

- `urgent`: now minus 24 hours
- `high`: now minus 1 hour
- `normal`: now
- `low`: now plus 1 hour

The service in-memory queue also ranks sources:

- Founder/owner/admin and review-sensitive work MUST outrank ordinary queue
  work.
- Queue-guard work MAY pin to the front while repairing pressure.
- Runtime blocker backoff and working-hours holds override rank.

### 10.4 Queue Routing States

Canonical queue route statuses:

- `pending`
- `blocked`
- `failed`
- `handled`
- `cancelled`
- `leased`

The direct queue command accepts `pending`, `blocked`, `failed`, `handled`, and
`cancelled`. Leasing functions may set `leased`.

Terminal queue statuses include:

- `handled`
- `cancelled`
- `failed`
- `completed`

### 10.5 Queue Leasing

Queue leasing MUST set:

- route status `leased`
- lease owner
- `leased_at`
- `acked_at = NULL`

When a worker completes successfully, leased queue messages SHOULD be acked
`handled`. On failure, they SHOULD be acked `failed` unless a retryable founder
communication policy keeps them `pending` or `review_rework`.

## 11. Self-Work

### 11.1 Purpose

Self-work represents durable internal CTOX work that survives turns and can be
published or synchronized through any adapter-backed ticket system when
supported.

Use self-work when:

- The task is multi-turn.
- Approval, rework, reminders, or recovery are needed.
- Work remains after a turn and should not be just prose.
- Queue-only state would be too fragile.

### 11.2 Self-Work Fields

`ticket_self_work_items` store:

- `work_id`
- `source_system`
- `kind`
- `title`
- `body_text`
- `state`
- `metadata_json`
- `remote_ticket_id`
- `remote_locator`
- `created_at`
- `updated_at`

Assignments and notes are stored in:

- `ticket_self_work_assignments`
- `ticket_self_work_notes`

### 11.3 Commands

Self-work commands:

- `ctox ticket self-work-put`
- `ctox ticket self-work-publish`
- `ctox ticket self-work-assign`
- `ctox ticket self-work-note`
- `ctox ticket self-work-transition`
- `ctox ticket self-work-list`
- `ctox ticket self-work-show`

`--publish` MAY create a remote ticket if the adapter supports
`can_create_self_work_items`.

### 11.4 Routing Assigned Self-Work

The service router SHOULD route published self-work when:

- state is `published`
- latest assignment is to `self`
- no suppression rule supersedes it

Routing self-work creates a queue task backed by the work item. The queue task
MUST carry `ticket_self_work_id` and metadata such as thread key, workspace
root, priority, suggested skill, parent message key, origin source, and dedupe
key when present.

### 11.5 Self-Work Completion

On successful worker completion:

- Completion review MAY approve, hold, rewrite, require rework, or request
  continuation.
- Approved self-work SHOULD be transitioned/closed with an internal note.
- Rejected work SHOULD be requeued without creating duplicate nested work.
- Timeout continuation SHOULD supersede or continue the work with a durable
  follow-up.

## 12. Queue and Ticket Bridge

CTOX may spill or restore queue work into ticket self-work using
`queue_ticket_spills`.

Bridge fields:

- `message_key`
- `work_id`
- `ticket_system`
- `bridge_state`
- `spilled_at`
- `restored_at`
- `updated_at`

Implementations SHOULD use bridge records when queue pressure, long-running
work, or rework makes plain queue state insufficient. Founder/owner/admin
communication MUST NOT be silently superseded by lower-priority queue spill.

## 13. Ticket-to-Queue Integration Contract

### 13.1 Integration Boundary

CTOX's queue and core state machine own orchestration. The built-in ticket
subsystem owns ticket integration facts and control evidence for work that came
from, or must write back to, a synchronized ticket system.

Ticket-sourced work SHOULD enter the queue only after CTOX has created durable
ticket context. Queue tasks SHOULD carry references back to the durable context
they depend on.

Durable context anchors include:

- ticket case
- ticket self-work item
- plan step
- schedule task
- communication message
- mission watchdog follow-up
- timeout continuation
- queue repair task

Queue tasks derived from ticket work MUST preserve enough metadata to reconnect
the runnable slice to its ticket context:

- `ticket_self_work_id` for self-work-backed queue tasks.
- `parent_message_key` for rework, continuation, and repair chains.
- `thread_key` for continuity routing.
- `workspace_root` when execution must resume in a specific workspace.
- `suggested_skill` when source skill resolution selected one.
- `dedupe_key` when a continuation/rework loop could otherwise duplicate work.
- `origin_source_label` when the queue item represents protected work from
  another channel.

### 13.2 Ticket Event Dispatch Contract

An inbound ticket event MUST NOT be queued as ordinary work until it has passed
ticket preparation.

Preparation MUST produce:

- a mirrored `ticket_items` row
- a mirrored `ticket_events` row
- a `ticket_event_routing_state` lease
- a source-control row for the ticket system
- a successful knowledge load with no required-domain gaps
- a label or synthetic self-work route
- an effective control bundle and autonomy resolution
- a dry-run artifact
- a `ticket_cases` row
- a prompt that references the case, dry run, ticket key, control mode, and
  expected writeback path

If any preparation step fails, the service MUST NOT create a runnable queue
slice. It MUST block or fail the ticket event route with governance evidence.

### 13.3 Queue Slice Lifecycle for Ticket Work

A queue slice derived from ticket work MAY complete, fail, block, be held for
review, or request continuation. It MUST update the related CTOX runtime and
ticket context accordingly:

- Successful ticket event handling MUST acknowledge the ticket event as
  `handled`.
- Worker failure MUST acknowledge the ticket event as `failed` unless a
  documented retry policy keeps it pending.
- Missing source knowledge or control state MUST acknowledge the ticket event as
  `blocked`.
- Approved self-work completion SHOULD transition the self-work toward a closed
  or completed state with an internal note.
- Review rejection SHOULD requeue the same durable self-work without nesting new
  self-work under self-work.
- Timeout continuation SHOULD create or reuse a durable continuation with a
  dedupe key.

### 13.4 Workspace Contract

Queue slices MAY carry a `workspace_root`. When present:

- The value MUST be normalized before use.
- Continuations and rework SHOULD preserve it.
- Prompt-derived workspace hints are legacy compatibility; explicit metadata
  SHOULD win.
- A worker MUST NOT run in an unsafe or unintended workspace path.

If workspace metadata is missing, the worker MAY use the implementation default
workspace behavior, but it SHOULD record enough context for recovery and
continuation.

## 14. Service Routing and Worker Lifecycle

### 14.1 Routing Preconditions

The service router MUST NOT route external messages while:

- queue pressure repair is active
- an agent loop is in progress

Before routing, the service SHOULD:

1. Route assigned self-work.
2. Repair stalled founder communication.
3. Emit due schedules.
4. Sync configured ticket systems.
5. Lease inbound communication messages.
6. Route ticket events.

### 14.2 Ticket Event to Worker Flow

Ticket event flow:

1. `sync_configured_ticket_systems`
2. `ticket_translation::apply_ticket_sync_batch`
3. `ensure_ticket_event_routing_rows`
4. `lease_pending_ticket_events`
5. `prepare_ticket_event_for_prompt`
6. `render_ticket_prompt`
7. `enqueue_prompt`
8. `start_prompt_worker`
9. completion review and durable state update
10. `ack_leased_ticket_events`

### 14.3 Queue Work to Worker Flow

Queue flow:

1. Queue task is created by TUI, chat, plan, schedule, timeout, review,
   self-work, queue repair, or another producer.
2. Queue task is represented as a `communication_messages` row.
3. Routing state is `pending`.
4. The service leases the queue task.
5. `QueuedPrompt` carries prompt, source label, thread key, workspace root,
   suggested skill, and leased message keys.
6. The worker runs a turn through `turn_loop::run_chat_turn_with_events_extended`.
7. The worker acks or requeues the message according to outcome and review.

### 14.4 Working Hours and Runtime Backoff

If working-hours policy rejects work, the service MUST keep the prompt queued
and record a visible event.

If a hard runtime blocker cooldown is active, the service MUST keep new prompts
queued, record governance event `runtime_blocker_backoff`, and resume when the
cooldown clears.

### 14.5 Timeout Continuation

If an agent turn times out, CTOX SHOULD create a timeout continuation self-work
or queue task with enough metadata to resume the same mission, workspace, thread
key, and parent task. The continuation mechanism SHOULD avoid reusing the
current leased message as the new pending task.

### 14.6 Dispatch Preflight

Before routing new work, CTOX SHOULD run a lightweight preflight. Preflight
MUST NOT mutate external ticket systems. It MAY record governance or service
events.

Preflight SHOULD verify:

- runtime SQLite store is open and migrated
- service is inside working hours or work is held
- queue pressure repair is not active
- active worker loop is idle before external routing
- runtime blocker cooldown is absent or work remains queued
- configured ticket systems are known adapter names
- configured adapter credentials are present when required
- adapter capability declarations can be loaded
- enabled model/runtime path is reachable enough to start a turn
- source knowledge and control gates will block before active handling when
  incomplete

If preflight fails for a source, CTOX SHOULD skip dispatch from that source for
the cycle and keep unrelated reconciliation active.

### 14.7 Reconciliation

Reconciliation MUST make durable state consistent with observable runtime state.
It SHOULD run before new external routing and after worker completion.

Reconciliation SHOULD inspect:

- leased queue tasks with no active worker
- leased ticket events with no active worker
- self-work assigned to `self` but not queued
- self-work marked `queued` with no runnable queue task
- stale founder communication rework
- stale plan routes
- timeout continuations with duplicate pending queue tasks
- blocked ticket events whose missing knowledge has since been captured
- runtime blocker cooldown expiry
- queue pressure guard state

Allowed reconciliation actions:

- release a stale lease to `pending`
- keep protected work held until its guard clears
- mark duplicates `duplicate` or `cancelled`
- block work with a durable note
- restore spilled queue work from self-work
- queue assigned self-work
- record governance evidence
- request queue repair instead of mutating ambiguous state

Reconciliation MUST NOT silently close ticket cases, self-work, or founder
communication without the required review and verification evidence.

## 15. Core State Machine

### 15.1 Entities

Core entity types:

- `service`
- `mission`
- `context`
- `queue_item`
- `work_item`
- `ticket`
- `review`
- `founder_communication`
- `commitment`
- `schedule`
- `knowledge`
- `repair`

### 15.2 Runtime Lanes

Runtime lanes:

- `p0_founder_communication`
- `p0_commitment_backing`
- `p1_runtime_safety`
- `p1_queue_repair`
- `p2_mission_delivery`
- `p3_housekeeping`

Protected work MUST run in the correct lane. Founder, owner, and admin
communication MUST use `p0_founder_communication`.

### 15.3 Queue Item Core Transitions

Allowed queue item transitions:

- `Pending -> Leased`
- `Leased -> Pending`
- `Leased -> Running`
- `Leased -> Completed`
- `Leased -> Blocked`
- `Leased -> Failed`
- `Running -> Completed`
- `Running -> Blocked`
- `Running -> Failed`
- `Running -> Superseded`
- `Blocked -> Pending`
- `Blocked -> Superseded`
- `Failed -> Pending`
- `Failed -> Superseded`
- `Pending -> Superseded`
- `Leased -> Superseded`

### 15.4 Ticket and Work Item Core Transitions

Allowed ticket/work item transitions:

- `Created -> Classified`
- `Created -> Planned`
- `Created -> Superseded`
- `Classified -> Planned`
- `Classified -> TicketBacked` for work items
- `Classified -> Superseded`
- `TicketBacked -> Planned` for work items
- `TicketBacked -> Superseded` for work items
- `Planned -> Executing`
- `Planned -> Superseded`
- `Executing -> AwaitingReview`
- `AwaitingReview -> ReworkRequired`
- `AwaitingReview -> AwaitingVerification`
- `AwaitingReview -> Superseded`
- `ReworkRequired -> Executing`
- `ReworkRequired -> Superseded`
- `AwaitingVerification -> Verified`
- `Verified -> Closed`
- `Executing -> Blocked`
- `Executing -> Superseded`
- `Blocked -> Planned`
- `Blocked -> Superseded`

### 15.5 Guard Rules

The transition guard MUST reject:

- Invalid transitions for an entity type.
- Founder/owner/admin communication outside the P0 founder lane.
- Founder/owner/admin communication supersession by lower-priority work.
- Founder/owner/admin sends without durable review audit and matching approved
  body/recipient hashes.
- Owner-visible completion closure without durable review audit.
- Ticket or self-work closure without verification evidence.
- Commitment activation without backing schedule task.
- Pausing/disabling a commitment-backed schedule without replacement or
  escalation.
- Repair transitions without a canonical hot path.
- Knowledge activation without an incident link.

### 15.6 Liveness

The core state machine MUST remain live:

- No unreachable states.
- No nonterminal dead-end states.
- Every nonterminal state has a path to a terminal state.

Implementations SHOULD run liveness analysis in tests when adding entity states
or transitions.

## 16. Review, Verification, and Writeback Safety

### 16.1 Completion Review

After successful worker execution, CTOX SHOULD run a separate completion review
for high-risk or owner-visible work. The reviewer MAY return:

- approve
- hold
- no-send
- rewrite-only
- requeue self-work
- continue self-work

Review failure SHOULD NOT be treated as proof of work completion.

### 16.2 Verification

Ticket and self-work closure MUST be backed by durable verification evidence.

Verification evidence SHOULD include:

- verification ID
- verification status
- summary
- referenced command, artifact, or external observation when applicable

### 16.3 Writeback Control Notes

Transition writebacks MAY include control notes with:

- schema
- operation
- case ID
- ticket key
- label
- bundle label and version
- approval mode
- autonomy level
- support mode
- risk level
- verification status
- verification summary

Adapters SHOULD write these as internal notes when the external system supports
internal comments.

## 17. Learning Loop

Learning candidates allow CTOX to improve future operation without silently
raising autonomy.

Creation command:

```text
ctox ticket learn-candidate-create --case-id <id> --summary <text> [--actions <json-array>] [--evidence-json <json>]
```

Decision command:

```text
ctox ticket learn-candidate-decide --candidate-id <id> --status <approved|rejected> [--decided-by <actor>] [--notes <text>] [--promote-autonomy-level <level>]
```

Rules:

- Candidates start as `proposed`.
- Only explicit decisions change candidate status.
- Autonomy promotion MUST be stored as a decision result and MUST NOT mutate
  control bundles implicitly.

## 18. CLI Reference Surface

The automated ticket system MUST expose at least these command groups:

Ticket source:

- `ctox ticket init`
- `ctox ticket sync --system <system>`
- `ctox ticket test --system <system>`
- `ctox ticket capabilities --system <name>`
- `ctox ticket sources`

Ticket knowledge and skills:

- `ctox ticket source-skills`
- `ctox ticket source-skill-set`
- `ctox ticket source-skill-show`
- `ctox ticket source-skill-query`
- `ctox ticket source-skill-import-bundle`
- `ctox ticket source-skill-resolve`
- `ctox ticket source-skill-compose-reply`
- `ctox ticket source-skill-review-note`
- `ctox ticket history-export`
- `ctox ticket knowledge-bootstrap`
- `ctox ticket knowledge-list`
- `ctox ticket knowledge-show`
- `ctox ticket knowledge-load`
- `ctox ticket monitoring-ingest`

Self-work:

- `ctox ticket access-request-put`
- `ctox ticket self-work-put`
- `ctox ticket self-work-show`
- `ctox ticket self-work-publish`
- `ctox ticket self-work-assign`
- `ctox ticket self-work-note`
- `ctox ticket self-work-transition`
- `ctox ticket self-work-list`

Ticket event handling:

- `ctox ticket take`
- `ctox ticket ack`
- `ctox ticket list`
- `ctox ticket show`
- `ctox ticket history`

Control:

- `ctox ticket label-set`
- `ctox ticket label-show`
- `ctox ticket bundle-put`
- `ctox ticket bundle-list`
- `ctox ticket autonomy-grant-set`
- `ctox ticket autonomy-grant-list`
- `ctox ticket dry-run`
- `ctox ticket cases`
- `ctox ticket case-show`
- `ctox ticket approve`
- `ctox ticket execute`
- `ctox ticket verify`
- `ctox ticket writeback-comment`
- `ctox ticket writeback-transition`
- `ctox ticket close`
- `ctox ticket audit`

Local adapter:

- `ctox ticket local init`
- `ctox ticket local create`
- `ctox ticket local comment`
- `ctox ticket local transition`
- `ctox ticket local list`
- `ctox ticket local show`

Queue:

- `ctox queue add`
- `ctox queue list`
- `ctox queue show`
- `ctox queue edit`
- `ctox queue block`
- `ctox queue release`
- `ctox queue complete`
- `ctox queue fail`
- `ctox queue cancel`
- queue spill/restore/repair commands as implemented by `src/mission/queue.rs`

Harness flow:

- `ctox harness-flow events`
- support bundle commands as implemented by `src/service/harness_flow.rs`

## 19. Observability and Recovery

### 19.1 Audit

Every material ticket control action SHOULD record `ticket_audit_log` with:

- ticket key
- case ID when applicable
- actor type
- action type
- label and bundle information when applicable
- details JSON
- timestamp

### 19.2 Harness Flow

Harness-flow events SHOULD be recorded for:

- self-work creation
- self-work publication
- ticket event routing
- queue pickup
- ticket pickup
- knowledge load
- guard decisions
- verification and writeback

Events MUST include enough keys to reconstruct the chain:

- `message_key`
- `work_id`
- `ticket_key`
- `attempt_index`
- metadata

### 19.3 Recovery Rules

On restart, CTOX SHOULD recover from durable state rather than transient memory:

- Pending and leased communication rows remain queryable.
- Ticket events with `pending` or compatible `leased` status can be leased.
- Self-work remains durable and may be routed again if still assigned to self.
- Queue repair MAY release, block, cancel, or reprioritize stale work.
- Timeout continuations and watchdog tasks SHOULD carry parent and dedupe
  metadata to avoid duplicate loops.

## 20. Failure Model

Failures MUST be converted into explicit durable state, not hidden in worker
prose.

| Failure class | Required CTOX behavior |
| --- | --- |
| Adapter sync failure | Record an operator-visible error; skip dispatch for that source; keep unrelated routing active. |
| Adapter auth/config missing | Fail source preflight; do not attempt remote writes; surface missing runtime/secret state. |
| Source adoption missing | Create `ticket_source_controls` during sync before active handling. |
| Required knowledge missing | Block the ticket event with `ticket_knowledge_gate`; do not create a runnable queue slice. |
| Label/control bundle missing | Block the ticket event with `ticket_control_gate`; do not create a runnable queue slice. |
| Approval missing | Keep the case `approval_pending`; execution commands must reject it. |
| Dry-run-only bundle | Create dry-run/case evidence; keep execution blocked. |
| Worker turn failure | Mark leased queue messages or ticket events `failed`, unless a documented retryable policy keeps them pending. |
| Worker turn timeout | Create or reuse timeout continuation self-work/queue state with dedupe metadata. |
| Runtime blocker | Hold new prompts in `pending_prompts`; record `runtime_blocker_backoff`; resume after cooldown. |
| Completion review rejection | Preserve durable self-work and requeue corrective work without nested duplicates. |
| Verification failure | Record failed verification and move the case to `blocked`. |
| Writeback capability missing | Fail before remote mutation; keep case ready for a supported writeback or operator action. |
| Remote writeback failure | Keep case open in writeback-capable state; record writeback/audit failure evidence. |
| Stale lease | Reconciliation may release, block, cancel, or repair according to source and protection level. |
| Queue pressure | Pause external routing; run queue repair; protect canonical hot path. |
| Founder/owner/admin send failure | Keep protected communication open or in delivery repair; never mark sent without send evidence. |

Failure handling SHOULD prefer the least destructive durable action:

1. Hold when policy says work is temporarily ineligible.
2. Block when missing knowledge, approval, control, or verification is required.
3. Requeue when a bounded retry or corrective slice is known.
4. Fail when the attempted slice failed and no safe automatic retry is known.
5. Supersede only when durable evidence identifies replacement work.
6. Close only after required verification and review gates pass.

## 21. Test and Validation Matrix

A conforming implementation SHOULD test the following behavior.

### 21.1 Adapter Sync and Normalization

- Sync creates or updates `ticket_items`.
- Sync creates or updates `ticket_events`.
- First sync creates `ticket_source_controls`.
- Remote outbound event marks prevent self-generated writebacks from re-routing
  as new inbound work.
- Adapter capability checks reject unsupported writes before remote mutation.

### 21.2 Source Knowledge and Control

- Missing required knowledge domains block dry-run creation.
- Knowledge load records loaded and missing domains.
- Label assignment selects the expected control bundle.
- Synthetic self-work label routing works for published self-work tickets.
- Autonomy grant resolution chooses the correct effective approval mode and
  autonomy level.

### 21.3 Case Lifecycle

- Dry run creates `ticket_cases`, `ticket_dry_runs`, and audit records.
- Human approval moves a case to `executable`; rejection moves it to `blocked`.
- Execution rejects non-executable cases.
- Verification passed moves a case to `writeback_pending`.
- Verification failed moves a case to `blocked`.
- Closure fails without passed verification evidence.
- Closure fails without owner-visible review evidence when required.

### 21.4 Ticket Event Routing

- Lease selects oldest pending inbound events.
- Ack `handled`, `duplicate`, and `blocked` set `acked_at`.
- Ack `failed` clears the lease without setting `acked_at`.
- Preparation failure blocks the event and records governance evidence.
- Prepared ticket events enqueue prompts with case and dry-run context.

### 21.5 Queue and Self-Work

- Queue priority canonicalization rejects invalid values.
- Queue `sort_at` shifts urgent/high/normal/low work correctly.
- Queue leasing sets route status `leased` and clears `acked_at`.
- Self-work assigned to `self` becomes a queue task.
- Self-work queue tasks carry `ticket_self_work_id`, thread, workspace, skill,
  parent, and dedupe metadata when present.
- Review rejection requeues the same durable self-work without nested
  self-work.
- Timeout continuation creates or reuses durable continuation work.

### 21.6 Reconciliation and Recovery

- Stale queue leases are released or repaired.
- Stale ticket event leases are released, blocked, or failed according to
  policy.
- Published self-work assigned to `self` is routed after restart.
- Duplicate timeout continuations are suppressed by dedupe metadata.
- Founder/owner/admin work is not superseded by ordinary queue repair.
- Queue pressure pauses external routing until repair completes.

### 21.7 Core State Machine and Governance

- Invalid core transitions are rejected.
- Ticket/self-work closure without verification is rejected.
- Founder send without review audit or matching hashes is rejected.
- Commitment-backed schedule disable requires replacement or escalation.
- Repair transitions require canonical hot path evidence.
- Core liveness analysis has no unreachable or dead-end nonterminal states.

### 21.8 Worker and Review Path

- Worker success acks leased queue/ticket events according to source.
- Worker failure records structured `AgentOutcome`.
- Completion review approval permits eligible closure.
- Completion review hold leaves work open.
- Rewrite-only review queues a lightweight corrective retry.
- Review failure does not count as completion.

## 22. Security and Authority

### 22.1 Secrets

Ticket credentials MUST be stored through CTOX's secret/runtime configuration
system. Agents MUST NOT persist entrusted secrets in queue items, ticket notes,
plans, ordinary message text, shell profiles, or runtime config rows that are
not secret storage.

### 22.2 Sender and Founder Boundaries

Founder, owner, and admin communication is protected work:

- It MUST be ranked above ordinary work.
- It MUST NOT be spilled or superseded by lower-priority queue work.
- Outbound sends MUST pass review with durable body and recipient evidence.

### 22.3 Remote Writes

Remote ticket writes MUST be gated by:

- adapter capability check
- case state precondition
- approval/control state
- verification when closing
- audit and writeback record

## 23. Conformance Checklist

An implementation conforms to this specification when it:

- Uses `runtime/ctox.sqlite3` as the authoritative ticket and queue store.
- Treats CTOX's queue and core state machine as the orchestration authority.
- Treats CTOX's built-in ticket subsystem as the ticket integration and control
  layer for synchronized ticket systems.
- Provides adapter capability declarations for every ticket source.
- Normalizes ticket sync into `ticket_items`, `ticket_events`, and routing
  state.
- Creates `ticket_source_controls` on first sync.
- Enforces required ticket knowledge domains before dry-run-controlled
  execution.
- Resolves labels, bundles, effective approvals, autonomy, and dry runs before
  active handling.
- Routes ticket events through explicit leases and acknowledgements.
- Preserves ticket/self-work context metadata on queue slices.
- Runs dispatch preflight before routing source work.
- Runs reconciliation for stale leases, assigned self-work, queue pressure,
  runtime blockers, and continuation dedupe.
- Routes durable self-work through queue tasks instead of relying on prose.
- Runs worker execution through CTOX's turn loop and completion-review path.
- Converts failures into explicit durable states according to the failure
  model.
- Requires durable verification and review evidence for closure.
- Uses the core state-machine guard for protected transitions.
- Records audit and harness-flow evidence for recovery and review.
- Supports arbitrary ticket-system sync through adapters instead of baking in
  Linear-specific schema, state, config, or API requirements.
