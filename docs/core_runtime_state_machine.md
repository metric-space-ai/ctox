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
- **Terminal States**: `Done`, `Escalated`
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
  - `SendFailed` -> `DeliveryRepair`
  - `DeliveryRepair` -> `Sending`
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

## Enforcement Rules

### Safety Gates
The kernel asserts the following rules during `validate_transition` calls:

1. **Founder Protected Lane (`validate_founder_communication`)**:
   - Outbound founder communication (`Sending` / `Sent` states) **must** run in the `P0FounderCommunication` lane and requires a valid review audit.
   - Outbound founder emails are blocked if the message body or recipient list differs from the approved draft.
   - Founder items cannot be superseded or spilled to external ticket systems.
2. **Outcome Witnessing (`validate_outcome_witness`)**:
   - Moving to a terminal state (`Completed`, `Closed`, `Sent`, `Done`) requires explicit validation: all expected technical artifacts must exist in their expected terminal state. Missing deliverables raise a `WP-Outcome-Missing` violation.
3. **Required Reviews (`validate_review_gate`)**:
   - Owner-visible work closures require a durable review audit key and `completion_review_verdict=pass`.
   - Rejections during review checkpoints must resume the `main_agent` instead of spawning review-owned subtasks.
4. **Verification Gates (`validate_ticket_closure`)**:
   - WorkItem or Ticket closures require passing `verification_runs` evidence.
