# CTOX Core Runtime State Machine

This document defines the target state machine for the CTOX harness. It is
intentionally about long-running control logic, not about whether an agent
writes code well, chooses a good architecture, or creates a commit.

The goal is to make the runtime deterministic enough that the LLM cannot
accidentally skip review, forget founder communication, lose commitments,
close tickets without evidence, or get trapped in queue repair loops.

For the current review, spawner, subagent, and liveness-proof operating model,
see [Harness Operating Model](harness-operating-model.md). This state-machine
document defines the durable state shape; the operating model explains how the
harness uses it to keep review and task spawning bounded.

## Scope

In scope:

- runtime queue and leasing
- owner/founder communication
- review and rework gates
- commitments and scheduled backing tasks
- tickets, execution records, and verification
- knowledge capture
- LCM continuity and mission state
- queue repair and loop repair
- service health and runtime invariants

Out of scope:

- how the agent writes code
- how the agent chooses implementation details inside a work slice
- git branch, commit, and PR mechanics
- product-specific acceptance criteria except when they become owner-visible
  commitments

## Durable State Vector

Every runtime decision must be reducible to this durable state vector:

```text
RuntimeState =
  ServiceState
  MissionState
  ContextState
  QueueState
  WorkItemState
  TicketState
  ReviewState
  CommunicationState
  CommitmentState
  ScheduleState
  KnowledgeState
  RepairState
```

All state reads must come from SQLite-backed runtime state, service status, or
explicit live checks recorded back into SQLite. Prompt text alone is not
durable state.

## State Machines

### 1. Service State

```text
Booting -> Ready
Booting -> Degraded
Ready -> Processing
Processing -> Ready
Processing -> Degraded
Degraded -> Repairing
Repairing -> Ready
Repairing -> Degraded
Any -> Stopped
```

Required durable facts:

- service process identity
- active runtime DB path
- model/provider availability
- secret-store availability
- queue worker availability
- last successful scheduler tick
- last invariant audit

Forbidden states:

- `Ready` when the runtime DB path is missing or not writable.
- `Ready` when protected secrets are only available through process/global env
  and not through the encrypted SQLite secret store.
- `Processing` without a durable lease or direct-session run record.

### 2. Mission State

```text
Uninitialized -> Open
Open -> Active
Active -> Blocked
Active -> AwaitingReview
Active -> AwaitingVerification
AwaitingReview -> ReworkRequired
AwaitingReview -> AwaitingVerification
ReworkRequired -> Active
AwaitingVerification -> Active
AwaitingVerification -> Completed
Blocked -> Active
Blocked -> Escalated
Completed -> Archived
```

Required durable facts:

- mission statement
- active vision/strategy references
- current done gate
- current blocker, if any
- next bounded slice
- continuity focus head
- closure confidence

Forbidden states:

- `Completed` with open queue, open plan, open founder communication, open
  closure-blocking claim, or missing verification.
- `Blocked` without explicit missing condition and next recheck/escalation rule.
- `Active` with no next bounded slice.

### 3. Context State

```text
Missing -> Initialized
Initialized -> Fresh
Fresh -> Stale
Stale -> Refreshing
Refreshing -> Fresh
Refreshing -> Corrupt
Corrupt -> Repairing
Repairing -> Fresh
```

Required durable facts:

- continuity documents: focus, anchors, narrative
- continuity commit heads
- current mission state derived from continuity
- context health snapshot
- compaction/refresh trigger evidence

Forbidden states:

- LLM turn starts with `ContextState=Missing` for owner-visible work.
- Founder communication starts with stale context when newer inbound messages
  exist.
- Mission state diverges from continuity-derived state without an invariant
  violation.

### 4. Queue State

```text
Created -> Pending
Pending -> Leased
Leased -> Running
Running -> AwaitingReview
Running -> Blocked
Running -> Failed
Running -> Completed
AwaitingReview -> ReworkQueued
AwaitingReview -> Completed
ReworkQueued -> Pending
Blocked -> Pending
Blocked -> Escalated
Failed -> ReworkQueued
Failed -> Cancelled
Completed -> Handled
Pending -> Spilled
Spilled -> TicketBacked
TicketBacked -> Restored
Restored -> Pending
```

Required durable facts:

- message key
- thread key
- priority
- route status
- lease owner and leased-at timestamp
- parent message key, if any
- self-work or ticket backing, if spilled
- last status note

Forbidden states:

- `Leased` beyond lease TTL without heartbeat or recovery action.
- `Handled` if the work is owner-visible and review failed, was skipped, or is
  unavailable.
- `Spilled` for founder/owner communication. Founder/owner communication is
  P0 and non-spillable.
- `Cancelled` without an audit reason.
- Queue repair mutating arbitrary items without first identifying the canonical
  hot path.

### 5. Work Item State

```text
Draft -> Queued
Queued -> Assigned
Assigned -> InProgress
InProgress -> NeedsEvidence
NeedsEvidence -> AwaitingReview
AwaitingReview -> NeedsRework
AwaitingReview -> Verified
NeedsRework -> Queued
Verified -> Closed
InProgress -> Blocked
Blocked -> Queued
Blocked -> Escalated
```

Required durable facts:

- work id
- kind
- owner/source system
- body text
- linked queue task
- linked ticket, if externalized
- linked evidence records
- linked review or verification records

Forbidden states:

- `Closed` without verification evidence.
- `Closed` when generated review has `FAIL`, `PARTIAL`, or `UNAVAILABLE`.
- `NeedsRework` resolved by rephrasing alone when review findings require new
  evidence, implementation, communication repair, or live validation.

### 6. Ticket State

```text
New -> Triaged
Triaged -> Planned
Planned -> Executing
Executing -> AwaitingReview
AwaitingReview -> ReworkRequired
AwaitingReview -> AwaitingVerification
ReworkRequired -> Executing
AwaitingVerification -> Verified
Verified -> Closed
Executing -> Blocked
Blocked -> Planned
Blocked -> Escalated
```

Required durable facts:

- ticket key
- case id
- priority
- requester
- plan
- execution actions
- review outcome
- verification outcome
- closure reason

Forbidden states:

- `Closed` when `ticket_execution_actions` has no linked action for the ticket.
- `Closed` when `ticket_verifications` has no passing verification for the
  ticket.
- `Escalated` without an owner-visible blocker summary.

### 7. Review State

```text
NotRequired -> Skipped
Required -> ReviewQueued
ReviewQueued -> Reviewing
Reviewing -> Pass
Reviewing -> Fail
Reviewing -> Partial
Reviewing -> Unavailable
Fail -> ReworkRequired
Partial -> ReviewContinuation
ReviewContinuation -> Reviewing
Unavailable -> Blocked
Pass -> Approved
```

Required durable facts:

- reviewed artifact identity
- content hash
- thread key
- owner-visible flag
- review trigger reason
- reviewer identity
- verdict
- failed gates
- findings
- evidence
- rework requirement

Forbidden states:

- Owner/founder outbound with `NotRequired` or `Skipped`.
- `Approved` if artifact hash differs from the artifact that is sent.
- `Approved` without recipient and CC review for communication artifacts.
- `Pass` when any required gate is unverified.

Important correction:

Review triggering must not be keyword-score based for protected paths. The
following paths always require review:

- founder/owner/admin outbound communication
- public or buyer-visible claims
- deadline commitments
- queue repair plans
- ticket closure
- knowledge updates created after an incident
- scheduler changes for commitments
- secrets/runtime configuration changes

### 8. Founder Communication State

Founder communication is a protected state machine. It must not run through the
normal best-effort queue path.

```text
InboundObserved -> ContextLoaded
ContextLoaded -> ThreadClassified
ThreadClassified -> NeedsResponse
ThreadClassified -> NoResponseNeeded
NeedsResponse -> Drafting
Drafting -> DraftReady
DraftReady -> Reviewing
Reviewing -> ReworkRequired
Reviewing -> Approved
ReworkRequired -> EvidenceWork
EvidenceWork -> Drafting
Approved -> Sending
Sending -> Sent
Sending -> SendFailed
SendFailed -> DeliveryRepair
DeliveryRepair -> Sending
Sent -> AwaitingAcknowledgement
AwaitingAcknowledgement -> Done
AwaitingAcknowledgement -> FollowUpDue
FollowUpDue -> NeedsResponse
```

Required durable facts:

- inbound message key
- complete thread context snapshot id
- recipient and CC classification
- response requirement decision
- draft body hash
- review approval key
- approved body hash
- send result
- sent timestamp
- follow-up rule
- knowledge update after incidents or commitments

Forbidden states:

- `Sending` without `Approved`.
- `Sending` when `approved_body_hash != outgoing_body_hash`.
- `Sending` when recipient/CC set differs from the reviewed set.
- `Done` when latest founder inbound is still unanswered.
- `NoResponseNeeded` without explicit reason and reviewed classification.
- `Sent` without review audit row.
- Founder communication assigned normal priority or spilled.
- Founder communication hidden behind queue repair, strategy setup, or
  self-work cleanup.

### 9. Commitment State

```text
None -> Proposed
Proposed -> Reviewed
Reviewed -> Committed
Committed -> BackingScheduled
BackingScheduled -> DueSoon
DueSoon -> InProgress
InProgress -> Delivered
InProgress -> AtRisk
AtRisk -> Escalated
Delivered -> Verified
Verified -> Closed
Committed -> CancelledWithNotice
```

Required durable facts:

- commitment text
- owner/founder-visible source message
- deadline in canonical timezone
- local timezone interpretation
- backing schedule id
- latest preparation status
- delivery artifact
- founder update audit

Forbidden states:

- `Committed` without reviewed schedule backing.
- `BackingScheduled` when the scheduled task is disabled.
- `DueSoon` without active work item or escalation.
- `CancelledWithNotice` without founder/owner communication.
- Any deadline recorded only in free text without canonical UTC and local time.

### 10. Schedule State

```text
Created -> Enabled
Enabled -> Due
Due -> Emitted
Emitted -> BackingWorkQueued
BackingWorkQueued -> Acknowledged
Enabled -> Paused
Paused -> Enabled
Enabled -> Expired
Enabled -> DisabledByPolicy
```

Required durable facts:

- task id
- schedule name
- cron expression
- next run
- last run
- commitment id, if backing a commitment
- disable reason, if disabled

Forbidden states:

- Commitment backing task in `Paused`, `Expired`, or `DisabledByPolicy` without
  a replacement backing task or escalation.
- Manual or automatic disable without audit reason.
- Due task with no emitted run and no failure record.

### 11. Knowledge State

```text
Candidate -> EvidenceAttached
EvidenceAttached -> Reviewed
Reviewed -> Active
Reviewed -> Rejected
Active -> Superseded
Active -> NeedsRecheck
NeedsRecheck -> EvidenceAttached
IncidentObserved -> LessonDrafted
LessonDrafted -> Reviewed
Reviewed -> FailureShieldActive
```

Required durable facts:

- source system
- domain
- title
- summary
- content JSON
- evidence reference
- status
- supersession relationship, if any

Forbidden states:

- Incident handled without a failure-shield knowledge entry.
- Knowledge entry active without evidence.
- Repeated incident when an applicable failure shield exists but was not used.
- Runtime claim stored only as standalone markdown, not SQLite-backed knowledge.

### 12. Repair State

```text
Healthy -> PressureDetected
PressureDetected -> RepairPlanning
RepairPlanning -> RepairPlanReviewed
RepairPlanReviewed -> ApplyingDeterministicActions
ApplyingDeterministicActions -> RepairVerification
RepairVerification -> Healthy
RepairVerification -> StillDegraded
StillDegraded -> Escalated
```

Required durable facts:

- pressure signal
- canonical hot path
- stale/superseded items
- proposed deterministic actions
- applied actions
- verification result

Forbidden states:

- Repair applies actions without a plan.
- Repair plan is accepted without canonical hot path.
- Repair blocks founder/owner communication.
- Repair verification says healthy while open P0 communication exists.

## Global Invariants

These invariants must hold across all state machines.

### Safety Invariants

1. No founder/owner/admin outbound email is sent without a matching review audit.
2. No reviewed communication is sent if body hash or recipient set differs from
   review.
3. No founder communication is spillable.
4. No ticket or self-work item closes without verification.
5. No owner-visible completion claim closes with failed, partial, unavailable,
   or missing review.
6. No deadline commitment exists without enabled backing schedule.
7. No enabled schedule can be disabled without an audit reason and replacement
   or escalation when it backs a commitment.
8. No mission closes while open queue, plan, founder communication, or
   closure-blocking claim exists.
9. No knowledge incident closes without durable failure-shield entry.
10. No protected secret is read from global env when SQLite secret store is the
    configured source of truth.

### Liveness Invariants

1. Every founder inbound becomes `Done`, `NoResponseNeeded`, or `Escalated`
   within its SLA.
2. Every leased queue item either heartbeats, completes, blocks, fails, or is
   recovered after TTL.
3. Every commitment reaches `Delivered`, `AtRisk`, `Escalated`, or
   `CancelledWithNotice` before deadline.
4. Every failed review creates exactly one actionable rework item unless a
   newer equivalent item already exists.
5. Every repair pass either restores a canonical hot path or escalates.
6. Every incident creates or updates knowledge before being marked resolved.
7. Every accepted durable spawn has a registered parent-child contract and a
   finite budget.
8. Every accepted subagent path is depth-bounded, count-bounded, and leaf-only.
9. Review-gate rework must carry a witness of progress before the same artifact
   can re-enter the same review edge.

## Event Vocabulary

External events:

- `EmailInboundObserved`
- `JamiInboundObserved`
- `TuiInstructionObserved`
- `CronDue`
- `OwnerDeadlineApproaching`
- `ServiceStarted`
- `ServiceStopped`
- `ToolResultObserved`
- `LiveCheckObserved`

Internal events:

- `QueueCreated`
- `QueueLeased`
- `QueueHeartbeat`
- `QueueCompleted`
- `QueueFailed`
- `QueueBlocked`
- `ReviewRequested`
- `ReviewPassed`
- `ReviewFailed`
- `ReviewPartial`
- `ReworkCreated`
- `VerificationPassed`
- `VerificationFailed`
- `KnowledgePersisted`
- `CommitmentDetected`
- `ScheduleBacked`
- `InvariantViolationDetected`
- `RepairPlanCreated`
- `RepairActionsApplied`

Every event that changes durable state must write an audit row. Audit rows must
include actor, source event, previous state, next state, and reason.

## Protected Lanes

The runtime must maintain separate lanes:

| Lane | Purpose | Spillable | Requires review | Queue priority |
|---|---|---:|---:|---|
| P0 Founder communication | Founder/owner/admin communication | no | always | urgent |
| P0 Commitment backing | Deadline preparation and delivery | no | always before external send | urgent |
| P1 Runtime safety | secrets, scheduler, service health, DB integrity | no | for state-changing fixes | high |
| P1 Queue repair | unblock canonical hot path | no | plan and verification | high |
| P2 Mission delivery | normal CTO work | yes, only to tickets | conditional |
| P3 Housekeeping | cleanup, summaries, low-risk notes | yes | conditional |

## SQL Assertion Surface

The first implementation should expose read-only assertions for these checks:

```sql
-- Founder outbound without review.
SELECT message_key
FROM communication_messages
WHERE direction = 'outbound'
  AND channel = 'email'
  AND (
    recipient_addresses_json LIKE '%michael.welsch@metric-space.ai%'
    OR recipient_addresses_json LIKE '%o.schaefers@gmx.net%'
    OR cc_addresses_json LIKE '%michael.welsch@metric-space.ai%'
    OR cc_addresses_json LIKE '%o.schaefers@gmx.net%'
  )
  AND NOT EXISTS (
    SELECT 1
    FROM communication_founder_reply_reviews r
    WHERE r.sent_at IS NOT NULL
      AND communication_messages.observed_at BETWEEN r.sent_at AND datetime(r.sent_at, '+10 seconds')
  );
```

```sql
-- Disabled backing schedule for active commitment.
SELECT task_id, name
FROM scheduled_tasks
WHERE name LIKE '%founder%'
  AND enabled = 0
  AND last_run_at IS NULL;
```

```sql
-- Closed self-work without verification.
SELECT work_id, kind, title
FROM ticket_self_work_items
WHERE state = 'closed'
  AND NOT EXISTS (
    SELECT 1
    FROM verification_runs
    WHERE verification_runs.goal LIKE '%' || ticket_self_work_items.work_id || '%'
  );
```

```sql
-- Open founder inbound not handled.
SELECT cm.message_key, cm.subject, cr.route_status
FROM communication_messages cm
JOIN communication_routing_state cr USING (message_key)
WHERE cm.direction = 'inbound'
  AND cm.channel IN ('email', 'jami', 'tui')
  AND (
    cm.sender_address LIKE '%michael.welsch%'
    OR cm.sender_address LIKE '%schaefers%'
    OR cm.cc_addresses_json LIKE '%cto1@metric-space.ai%'
  )
  AND cr.route_status NOT IN ('handled', 'cancelled', 'approval-nag-handled');
```

These SQL checks are intentionally imperfect first cuts. The Rust invariant
layer must replace string matching with persisted participant roles,
commitment IDs, review artifact IDs, and explicit state rows.

## Required Runtime Tables

Current tables are not enough. The target model needs additional canonical
tables or equivalent columns:

- `core_state_events`
- `core_state_transitions`
- `core_invariant_violations`
- `communication_artifact_reviews`
- `communication_protected_threads`
- `commitments`
- `commitment_backing_tasks`
- `ticket_execution_actions`
- `ticket_verifications`
- `knowledge_failure_shields`
- `lease_heartbeats`
- `repair_runs`
- `repair_actions`

The existing tables may remain, but state transitions must be centralized
through a core transition API.

## Transition API Contract

All state changes must go through one API:

```text
transition(entity_type, entity_id, event, actor, evidence_refs, reason)
  -> validates preconditions
  -> writes old_state/new_state
  -> writes audit event
  -> applies deterministic side effects
  -> runs invariant checks
  -> blocks forbidden state
```

Direct SQL updates to protected state are forbidden outside migrations and
explicit repair tools.

## Minimal Implementation Order

1. Add typed state enums and transition validator for communication, schedule,
   queue, work item, ticket, review, commitment, and knowledge.
2. Add `core_state_events` and `core_invariant_violations`.
3. Gate founder outbound through the communication state machine.
4. Gate deadline commitments through commitment and schedule machines.
5. Gate self-work and ticket closure through review and verification machines.
6. Add invariant auditor command and service tick.
7. Add scenario tests for every forbidden state listed above.

## Methodical Hardening Plan

The state machine becomes production-safe in four layers. Each layer must be
testable without an LLM.

### Layer 1: Pure transition kernel

- implement typed enums for entities, states, events, lanes, and evidence refs
- reject invalid transitions before any side effect
- reject missing evidence for protected transitions
- keep the validator pure so it can be exhaustively tested

### Layer 2: SQLite transition ledger

- persist every accepted transition in `core_state_events`
- persist every rejected protected transition in `core_invariant_violations`
- record `old_state`, `new_state`, `event`, `actor`, `lane`, evidence refs, and
  body/recipient hashes for communication
- forbid direct protected-state mutation outside migrations and explicit repair
  tools

### Layer 3: Adapter gates

Every existing harness path must call the transition kernel before doing work:

- `channel founder-send`: `Approved -> Sending -> Sent`
- generic `channel send`: blocked for founder/owner/admin recipients
- inbound sync: `InboundObserved -> ContextBuilt -> ReplyNeeded`
- scheduler tick: `Due -> Emitted -> BackingWorkQueued -> Acknowledged`
- commitment creation: `Reviewed -> Committed -> BackingScheduled`
- ticket/self-work closure: `AwaitingVerification -> Verified -> Closed`
- queue repair: `RepairPlanning -> RepairPlanReviewed -> ... -> Restored`
- knowledge capture: `IncidentObserved -> ... -> Active`

### Layer 4: Proving and watchdogs

- unit tests enumerate all allowed transition tables
- forbidden-state tests cover every global invariant
- `ctox process-mining spawn-liveness` proves durable spawner contracts and
  harness subagent liveness
- replay tests rebuild state from `core_state_events` and compare with current
  materialized state
- SQL invariant tests run against fixture databases and the live runtime DB
- watchdogs turn liveness failures into P0/P1 queue items, never silent skips
- repair tools must name a canonical hot path and prove it is restored

If a protected behavior cannot be represented in this model, the model is
incomplete and the behavior must not be shipped as a hidden workaround.

## Scenario Test Matrix

The first complete test suite must cover:

| Scenario | Expected result |
|---|---|
| Founder inbound direct to CTO1 | P0 communication created, non-spillable |
| Founder inbound with CTO1 only in CC | Reply-all classification required |
| Founder outbound without review | blocked |
| Founder outbound with changed body after review | blocked |
| Founder outbound with changed recipients after review | blocked |
| Review fail requiring evidence work | rework item, no send |
| Review fail due wording only | rework item may stay in drafting |
| Commitment in approved email | schedule backing required |
| Commitment schedule disabled | invariant violation and escalation |
| Queue pressure with founder P0 item | founder item remains hot path |
| Queue repair plan without hot path | rejected |
| Ticket closed without execution action | blocked |
| Ticket closed without verification | blocked |
| Knowledge incident without failure shield | blocked |
| Service restart with leased items | expired leases recovered |
| LCM focus stale vs mission state | invariant violation |
| Secret requested from env | blocked when SQLite store is configured |

## Definition Of Done

This model is implemented only when:

- every protected transition goes through a typed transition API
- every forbidden state has a deterministic test
- every liveness rule has a watchdog or escalation path
- every owner/founder outbound has review audit evidence
- every incident creates durable knowledge
- queue repair cannot suppress P0 communication
- scheduler cannot silently disable commitment backing
- SQLite forensics can explain why every protected state changed
