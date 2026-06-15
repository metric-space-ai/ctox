# CTOX Core Runtime State Machine

This document defines the exact, executable state machine implemented in [core_state_machine.rs](file:///Users/michaelwelsch/Documents/ctox.nosync/src/core/service/core_state_machine.rs). It represents the true architectural constraints guarding CTOX daemon execution.

All state transitions, entity types, allowed state mappings, and terminal proofs are enforced programmatically by the transition kernel.

## Core Entity Types

Every state-machine actor belongs to one of these types:

- **Service**: Persistent CTOX daemon and its worker loop
- **Mission**: Orchestrated objective lifecycle
- **Context**: Long-context memory (LCM) hydration and compaction state
- **QueueItem**: Executable work unit in the persistent queue
- **WorkItem**: Bounded technical task assigned to workers
- **Ticket**: Integrated external case (e.g. Zammad tracker)
- **Review**: Quality gate audit checking drafts and artifacts
- **FounderCommunication**: Inbound/outbound mail and chat channel routing
- **Commitment**: Owner-visible deadline commitments
- **Schedule**: Periodic backing task emissions (Cron ticks)
- **Knowledge**: Retained incident history and failure shields
- **Repair**: Deterministic queue and lock recovery runs

## Allowed State Transitions

### 1. Service State
Guards service process liveness, DB readiness, and background loops.
- **Start State**: `Booting`
- **Terminal States**: `Ready`, `Stopped`
- **Valid Transitions**:
  - `Booting` -> `Ready`
  - `Booting` -> `Degraded`
  - `Ready` -> `Processing`
  - `Processing` -> `Ready`
  - `Processing` -> `Degraded`
  - `Degraded` -> `Repairing`
  - `Repairing` -> `Ready`
  - `Repairing` -> `Degraded`
  - `Booting` | `Ready` | `Processing` | `Degraded` | `Repairing` -> `Stopped`

### 2. Mission State
Tracks high-level project goals and Done gates.
- **Start State**: `Empty`
- **Terminal States**: `MissionReady`, `MissionClosed`
- **Valid Transitions**:
  - `Empty` -> `Ingesting`
  - `Ingesting` -> `Rebuilding`
  - `Rebuilding` -> `MissionReady`
  - `MissionReady` -> `MissionRunning`
  - `MissionRunning` -> `WaitingOnExternal`
  - `MissionRunning` -> `MissionBlocked`
  - `WaitingOnExternal` -> `MissionRunning`
  - `MissionBlocked` -> `Repairing`
  - `Repairing` -> `MissionRunning`
  - `MissionRunning` -> `MissionClosed`

### 3. Context State
Hydrates long-context memory (LCM) focus and controls emergency/adaptive compaction.
- **Start State**: `Cold`
- **Terminal States**: `Fresh`
- **Valid Transitions**:
  - `Cold` -> `Hydrating`
  - `Hydrating` -> `Fresh`
  - `Fresh` -> `CompactionDue`
  - `CompactionDue` -> `Compacted`
  - `Compacted` -> `Fresh`
  - `Fresh` -> `Stale`
  - `Stale` -> `Hydrating`

### 4. QueueItem State
Orchestrates durable queueing, leasing, heartbeats, and worker limits.
- **Start State**: `Pending`
- **Terminal States**: `Completed`, `Superseded`
- **Valid Transitions**:
  - `Pending` -> `Completed` | `Blocked` | `Failed` | `Leased` | `ReworkRequired`
  - `Leased` -> `Pending` | `Running` | `Completed` | `Blocked` | `Failed` | `ReworkRequired`
  - `Running` -> `Completed` | `Blocked` | `Failed` | `ReworkRequired` | `Superseded`
  - `Blocked` -> `Pending` | `Completed` | `Failed` | `ReworkRequired` | `Superseded`
  - `Failed` -> `Pending` | `Blocked` | `Completed` | `ReworkRequired` | `Superseded`
  - `ReworkRequired` -> `Pending` | `Failed` | `Superseded`
  - `Pending` | `Leased` -> `Superseded`

### 5. WorkItem State
Tracks bounded operator work assigned to agent runs.
- **Start State**: `Created`
- **Terminal States**: `Closed`, `Failed`, `Superseded`
- **Valid Transitions**:
  - `Created` -> `Classified` | `Planned` | `Superseded`
  - `Classified` -> `TicketBacked` | `Superseded`
  - `TicketBacked` -> `Planned` | `Superseded`
  - `Planned` -> `Closed` | `Executing` | `Blocked` | `Failed` | `Superseded`
  - `Executing` -> `Closed` | `AwaitingReview` | `Failed` | `Blocked` | `Superseded`
  - `AwaitingReview` -> `ReworkRequired` | `AwaitingVerification` | `Closed` | `Failed` | `Superseded`
  - `ReworkRequired` -> `Closed` | `Executing` | `Failed` | `Superseded`
  - `AwaitingVerification` -> `Verified`
  - `Verified` -> `Closed`
  - `Blocked` -> `Closed` | `Failed` | `Planned` | `Superseded`

### 6. Ticket State
Interfaces with external issue trackers.
- **Start State**: `Created`
- **Terminal States**: `Closed`, `Superseded`
- **Valid Transitions**:
  - `Created` -> `Classified` | `AwaitingReview` | `Blocked` | `Planned` | `Superseded`
  - `Classified` -> `Planned` | `Superseded`
  - `Planned` -> `Blocked` | `Executing` | `Superseded`
  - `Executing` -> `Verified` | `AwaitingReview` | `Blocked` | `Superseded`
  - `AwaitingReview` -> `ReworkRequired` | `AwaitingVerification` | `Planned` | `Blocked` | `Superseded`
  - `ReworkRequired` -> `Executing` | `Superseded`
  - `AwaitingVerification` -> `Verified`
  - `Verified` -> `Closed`
  - `Blocked` -> `Planned` | `Superseded`

### 7. Review State
Durable audit gate checking drafted content and code artifacts.
- **Start State**: `Drafting`
- **Terminal States**: `Approved`, `Rejected`
- **Valid Transitions**:
  - `Drafting` -> `DraftReady`
  - `DraftReady` -> `Reviewing`
  - `Reviewing` -> `Approved`
  - `Reviewing` -> `Rejected`
  - `Reviewing` -> `SentBackForRework`
  - `SentBackForRework` -> `Drafting`

### 8. FounderCommunication State
High-priority, non-spillable lane for communication.
- **Start State**: `InboundObserved`
- **Terminal States**: `Done`, `Escalated`, `SendFailed`
  - `SendFailed` is a terminal **failure-hold**: CTOX never auto-resends founder
    mail. A failed send stays here pending manual operator recovery (verify
    delivery, then issue a fresh send — guarded against a blind duplicate by the
    EGRESS-2 stranded-send check). The dead `SendFailed -> DeliveryRepair ->
    Sending` recovery loop was removed (EGRESS-4) because no driver emitted it.
- **Valid Transitions**:
  - `InboundObserved` -> `InboundObserved` | `ContextBuilt`
  - `ContextBuilt` -> `ReplyNeeded` | `NoResponseNeeded`
  - `ReplyNeeded` -> `Drafting` | `Escalated`
  - `Drafting` -> `DraftReady`
  - `DraftReady` -> `Reviewing`
  - `Reviewing` -> `Approved` | `ReworkRequired`
  - `ReworkRequired` -> `ContextBuilt`
  - `Approved` -> `Sending`
  - `Sending` -> `Sent` | `SendFailed`
  - `Sent` -> `AwaitingAcknowledgement`
  - `AwaitingAcknowledgement` -> `Done`
  - `NoResponseNeeded` -> `Done`

### 9. Commitment State
Ensures deadline schedules and owner promises are fulfilled.
- **Start State**: `Proposed`
- **Terminal States**: `Delivered`, `Escalated`, `CancelledWithNotice`
- **Valid Transitions**:
  - `Proposed` -> `Reviewed`
  - `Reviewed` -> `Committed`
  - `Committed` -> `BackingScheduled`
  - `BackingScheduled` -> `DueSoon`
  - `DueSoon` -> `InProgress`
  - `InProgress` -> `Delivered`
  - `DueSoon` -> `AtRisk`
  - `InProgress` -> `AtRisk`
  - `AtRisk` -> `InProgress`
  - `AtRisk` -> `Escalated`
  - `Committed` -> `CancelledWithNotice`
  - `BackingScheduled` -> `CancelledWithNotice`

### 10. Schedule State
Controls periodic/deferred Cron triggers.
- **Start State**: `Created`
- **Terminal States**: `Acknowledged`, `Expired`, `DisabledByPolicy`
- **Valid Transitions**:
  - `Created` -> `Enabled`
  - `Enabled` -> `Due`
  - `Due` -> `Emitted`
  - `Emitted` -> `BackingWorkQueued`
  - `BackingWorkQueued` -> `Acknowledged`
  - `Enabled` -> `Paused`
  - `Paused` -> `Enabled`
  - `Enabled` -> `Expired`
  - `Paused` -> `DisabledByPolicy`
  - `Enabled` -> `DisabledByPolicy`

### 11. Knowledge State
Retains incident logs and activates failure-shield policies.
- **Start State**: `IncidentObserved`
- **Terminal States**: `Active`, `Superseded`
- **Valid Transitions**:
  - `IncidentObserved` -> `LessonDrafted`
  - `LessonDrafted` -> `AwaitingReview`
  - `AwaitingReview` -> `EvidenceAttached`
  - `EvidenceAttached` -> `Active`
  - `Active` -> `Superseded`

### 12. Repair State
Deterministic recovery loop for stuck or degraded states.
- **Start State**: `Healthy`
- **Terminal States**: `Restored`
- **Valid Transitions**:
  - `Healthy` -> `PressureDetected`
  - `PressureDetected` -> `RepairPlanning`
  - `RepairPlanning` -> `RepairPlanReviewed`
  - `RepairPlanReviewed` -> `ApplyingDeterministicActions`
  - `ApplyingDeterministicActions` -> `RepairVerification`
  - `RepairVerification` -> `Restored`
  - `RepairVerification` -> `StillDegraded`
  - `StillDegraded` -> `RepairPlanning`

## Generated Transition Catalog (machine-pinned)

The block below is generated from `allowed_transition_catalog`, `core_start_state`,
and `core_terminal_states` in `core_state_machine.rs` and pinned byte-for-byte by the
test `core_runtime_state_machine_doc_matches_catalog`. Do **not** hand-edit it: change
the catalog in code, run that test, and paste its expected block here. It guarantees this
canonical audit doc cannot silently drift from the executable transition kernel.

<!-- BEGIN GENERATED core-state-machine -->
```text
Service:
  start = Booting
  terminal = Ready, Stopped
  Booting -> Ready
  Booting -> Degraded
  Ready -> Processing
  Processing -> Ready
  Processing -> Degraded
  Degraded -> Repairing
  Repairing -> Ready
  Repairing -> Degraded
  Booting -> Stopped
  Ready -> Stopped
  Processing -> Stopped
  Degraded -> Stopped
  Repairing -> Stopped

Mission:
  start = Empty
  terminal = MissionReady, MissionClosed
  Empty -> Ingesting
  Ingesting -> Rebuilding
  Rebuilding -> MissionReady
  MissionReady -> MissionRunning
  MissionRunning -> WaitingOnExternal
  MissionRunning -> MissionBlocked
  WaitingOnExternal -> MissionRunning
  MissionBlocked -> Repairing
  Repairing -> MissionRunning
  MissionRunning -> MissionClosed

Context:
  start = Cold
  terminal = Fresh
  Cold -> Hydrating
  Hydrating -> Fresh
  Fresh -> CompactionDue
  CompactionDue -> Compacted
  Compacted -> Fresh
  Fresh -> Stale
  Stale -> Hydrating

QueueItem:
  start = Pending
  terminal = Completed, Superseded
  Pending -> Completed
  Pending -> Blocked
  Pending -> Failed
  Pending -> Leased
  Pending -> ReworkRequired
  Leased -> Pending
  Leased -> Running
  Leased -> Completed
  Leased -> Blocked
  Leased -> Failed
  Leased -> ReworkRequired
  Running -> Completed
  Running -> Blocked
  Running -> Failed
  Running -> ReworkRequired
  Running -> Superseded
  Blocked -> Pending
  Blocked -> Completed
  Blocked -> Failed
  Blocked -> ReworkRequired
  Blocked -> Superseded
  Failed -> Pending
  Failed -> Blocked
  Failed -> Completed
  Failed -> ReworkRequired
  Failed -> Superseded
  ReworkRequired -> Pending
  ReworkRequired -> Failed
  ReworkRequired -> Superseded
  Pending -> Superseded
  Leased -> Superseded

WorkItem:
  start = Created
  terminal = Closed, Failed, Superseded
  Created -> Classified
  Created -> Planned
  Created -> Superseded
  Classified -> TicketBacked
  Classified -> Superseded
  TicketBacked -> Planned
  TicketBacked -> Superseded
  Planned -> Closed
  Planned -> Executing
  Planned -> Blocked
  Planned -> Failed
  Planned -> Superseded
  Executing -> Closed
  Executing -> AwaitingReview
  Executing -> Failed
  AwaitingReview -> ReworkRequired
  AwaitingReview -> AwaitingVerification
  AwaitingReview -> Closed
  AwaitingReview -> Failed
  AwaitingReview -> Superseded
  ReworkRequired -> Closed
  ReworkRequired -> Executing
  ReworkRequired -> Failed
  ReworkRequired -> Superseded
  AwaitingVerification -> Verified
  Verified -> Closed
  Executing -> Blocked
  Executing -> Superseded
  Blocked -> Closed
  Blocked -> Failed
  Blocked -> Planned
  Blocked -> Superseded

Ticket:
  start = Created
  terminal = Closed, Superseded
  Created -> Classified
  Created -> AwaitingReview
  Created -> Blocked
  Created -> Planned
  Created -> Superseded
  Classified -> Planned
  Classified -> Superseded
  Planned -> Blocked
  Planned -> Executing
  Planned -> Superseded
  Executing -> Verified
  Executing -> AwaitingReview
  AwaitingReview -> ReworkRequired
  AwaitingReview -> AwaitingVerification
  AwaitingReview -> Planned
  AwaitingReview -> Blocked
  AwaitingReview -> Superseded
  ReworkRequired -> Executing
  ReworkRequired -> Superseded
  AwaitingVerification -> Verified
  Verified -> Closed
  Executing -> Blocked
  Executing -> Superseded
  Blocked -> Planned
  Blocked -> Superseded

Review:
  start = Drafting
  terminal = Approved, Rejected
  Drafting -> DraftReady
  DraftReady -> Reviewing
  Reviewing -> Approved
  Reviewing -> Rejected
  Reviewing -> SentBackForRework
  SentBackForRework -> Drafting

FounderCommunication:
  start = InboundObserved
  terminal = Done, Escalated, SendFailed
  InboundObserved -> InboundObserved
  InboundObserved -> ContextBuilt
  ContextBuilt -> ReplyNeeded
  ContextBuilt -> NoResponseNeeded
  ReplyNeeded -> Drafting
  Drafting -> DraftReady
  DraftReady -> Reviewing
  Reviewing -> Approved
  Reviewing -> ReworkRequired
  ReworkRequired -> ContextBuilt
  Approved -> Sending
  Sending -> Sent
  Sending -> SendFailed
  Sent -> AwaitingAcknowledgement
  AwaitingAcknowledgement -> Done
  NoResponseNeeded -> Done
  ReplyNeeded -> Escalated

Commitment:
  start = Proposed
  terminal = Delivered, Escalated, CancelledWithNotice
  Proposed -> Reviewed
  Reviewed -> Committed
  Committed -> BackingScheduled
  BackingScheduled -> DueSoon
  DueSoon -> InProgress
  InProgress -> Delivered
  DueSoon -> AtRisk
  InProgress -> AtRisk
  AtRisk -> InProgress
  AtRisk -> Escalated
  Committed -> CancelledWithNotice
  BackingScheduled -> CancelledWithNotice

Schedule:
  start = Created
  terminal = Acknowledged, Expired, DisabledByPolicy
  Created -> Enabled
  Enabled -> Due
  Due -> Emitted
  Emitted -> BackingWorkQueued
  BackingWorkQueued -> Acknowledged
  Enabled -> Paused
  Paused -> Enabled
  Enabled -> Expired
  Paused -> DisabledByPolicy
  Enabled -> DisabledByPolicy

Knowledge:
  start = IncidentObserved
  terminal = Active, Superseded
  IncidentObserved -> LessonDrafted
  LessonDrafted -> AwaitingReview
  AwaitingReview -> EvidenceAttached
  EvidenceAttached -> Active
  Active -> Superseded

Repair:
  start = Healthy
  terminal = Restored
  Healthy -> PressureDetected
  PressureDetected -> RepairPlanning
  RepairPlanning -> RepairPlanReviewed
  RepairPlanReviewed -> ApplyingDeterministicActions
  ApplyingDeterministicActions -> RepairVerification
  RepairVerification -> Restored
  RepairVerification -> StillDegraded
  StillDegraded -> RepairPlanning
```
<!-- END GENERATED core-state-machine -->

## Enforcement Rules

### Safety Gates
The transition kernel asserts the following programmatic safety gates during `validate_transition` calls:

1. **Founder Protected Lane (`validate_founder_communication`)**:
   - Outbound founder/owner/admin communication (`Sending` / `Sent` states) **must** run in the `P0FounderCommunication` lane.
   - Outbound founder emails are blocked if the outgoing message body hash or recipient list hash differs from the approved draft.
   - Founder items cannot be superseded or spilled to external ticket systems.
2. **Required Reviews (`validate_review_gate`)**:
   - Owner-visible completion claims (`Closed` / `Delivered` states) require a durable review audit key.
   - Review-required terminal success requires `completion_review_verdict=pass` and verification/validator evidence.
   - Rejections or rework required during review checkpoints must target the reviewed entity and resume the `main_agent` instead of spawning review-owned subtasks.
3. **Rework Required Verification (`validate_rework_required_gate`)**:
   - Moving to `ReworkRequired` requires a durable review rejection/checkpoint or validator rework witness.
4. **Harness Static Model Consistency (`validate_review_harness_static_model`)**:
   - Ensures the review harness state transitions have no cycles without consuming budget, terminal success ends in `Passed` via `ValidatorPass`, and `ReviewPassed` only enters the validation gate.
5. **Terminal Failure Reasons (`validate_terminal_failure_gate`)**:
   - Moving QueueItem, WorkItem, or Ticket to `Failed` (unless in `P3Housekeeping`) requires durable `failure_reason` and `failure_class` metadata.
6. **Work Success Validation (`validate_work_terminal_success_gate`)**:
   - Work success (`Completed` or `Closed`) requires completion review and validation proof, or an explicit terminal policy proof.
7. **Verification Gates (`validate_ticket_closure`)**:
   - Ticket or WorkItem closures require passing `verification_id` evidence.
8. **Commitment Schedule Backing (`validate_commitment_backing`)**:
   - Deadline commitments (`Committed`, `BackingScheduled`, `DueSoon`) require a backing schedule task id before they can activate.
9. **Schedule Backing Commitments (`validate_schedule_backing`)**:
   - A schedule backing a commitment cannot be paused or disabled without specifying a replacement schedule task or raising an escalation.
10. **Deterministic Repair Hot Path (`validate_repair`)**:
    - Repairing/Restoring transitions must specify the canonical protected hot path being repaired.
11. **Incident Linkage (`validate_knowledge_capture`)**:
    - Failure-shield knowledge entries must specify the `incident_id` preventing recurrence.
12. **Outcome Witnessing (`validate_outcome_witness`)**:
    - Moving to a terminal state (`Completed`, `Closed`, `Sent`, `Done`) requires explicit validation: all expected technical artifacts must exist in their expected terminal state. Missing deliverables raise a `WP-Outcome-Missing` violation.

### Runtime State Invariants
In addition to the transition kernel, `src/core/service/state_invariants.rs` evaluates overall runtime state health using five critical programmatic constraints:

1. **Active Work on Closed Mission (`closed_mission_with_open_runtime_work`)**:
   - Durable runtime work (open plans/queues) is strictly forbidden if the mission state is closed, done, dormant, or not open.
2. **Idle Allowed with Open Work (`idle_allowed_with_open_runtime_work`)**:
   - A mission cannot allow idle state (`allow_idle = true`) while there is still open durable runtime work.
3. **Mission Focus Head Mismatch (`mission_focus_head_mismatch`)**:
   - The stored mission state must be synchronized to the latest focus continuity head commit ID.
4. **Continuity Resync Required (`mission_state_requires_continuity_resync`)**:
   - Stored mission state cache values must exactly match the mission state derived from the latest continuity document.
5. **Semantic Focus Conflict (`focus_semantic_conflict`)**:
   - The focus continuity document must not contain duplicate or conflicting values for normalized semantic fields (`Mission`, `Mission state`, `Continuation mode`, `Trigger intensity`, `Current blocker`, `Next slice`, `Done gate`, `Closure confidence`).

