# RFC 0001: Outbound Review Pipeline — Loop-Freedom by Construction

**Status:** Draft
**Author:** michaelwelsch
**Discussion:** see PR
**Affects:** `src/service/core_state_machine.rs`,
`src/service/core_transition_guard.rs`, `src/mission/channels.rs`,
`src/service/service.rs`. The pre-existing local design doc
`docs/core_runtime_state_machine.md` is referenced for target shape;
because `/docs/` is currently gitignored upstream, this RFC sits at
top-level `rfcs/` to be trackable in a PR.

## 1. Motivation

Owner-/founder-visible outbound communication today can sit in a non-terminating
review/rework cycle. A concrete production case (CTOX deployment "INF Yoda",
2026-05-04) showed:

1. The drafter composed a substantively correct outbound (subject:
   "Vorschlag Tag-System fuer Lead-Funnel in Salesforce", recipients
   `j.kienzler@remcapital.de`, CC `j.cakmak@…`, `d.lottes@…`).
2. The body lived only as a CLI argument to `ctox chat … --to … --wait`.
   It was never persisted into `communication_messages`.
3. The provider call exited `1` after ~32 minutes (provider auth failure
   against an outdated endpoint).
4. The harness had no body anchor for retry. The next agent turn produced a
   different artifact (an internal status note), which the reviewer correctly
   rejected.
5. The generic `enqueue_review_rework` path has no per-entity attempt counter
   (the `FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD = 2` circuit-breaker only
   covers `founder-communication-rework`). The reject/recompose cycle ran
   without bound, accumulating self-work, routing rows, and orphaned queue
   tasks.

This RFC defines the target outbound state machine such that:

- every reviewer critique class has an explicit transition;
- the worker is always returned to drafting on a finding, but never via a path
  that can self-re-enter without a witness of progress;
- the absence of infinite reject/recompose cycles is provable by static
  liveness analysis of the transition graph **and** detectable in production
  by the existing `process_mining` and `harness_mining` tooling.

## 2. Target Shape (operator-confirmed)

Every protected outbound deliverable passes through review. Review can raise
critique findings that fall into three top-level classes — and the
**Staleness** class itself splits into three operationally distinct
sub-classes, because "the world moved" can mean very different things:

| Top class       | Reviewer means                                  | Worker pathway                                                                                                                                |
|-----------------|-------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------|
| **Wording**     | content is fine, language/tone/format is off    | **straight-forward in-process rewrite**: same `substance_pointer`, new `body_hash`. No new task, no new review run; converge body and resubmit. |
| **Substantive** | content / strategy / evidence is wrong          | **real rework task**, not "compose the same email differently". Worker enters an Evidence-Work phase (research, ticket investigation, mission-state update, strategy clarification), produces a *new* `substance_pointer`, only **then** drafts. Better wording cannot fix this class. |
| **Staleness**   | the world moved while the draft sat             | one of three sub-pathways depending on **what** moved (see below).                                                                            |

Staleness sub-classes:

| Sub-class               | Reviewer means                                                         | Worker pathway                                                                                                                              |
|-------------------------|------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------|
| **Stale-Refresh**       | new facts arrived on the relevant axes; integrate and resend           | update `world_pointer`, integrate new facts into body, send.                                                                                 |
| **Stale-Obsolete**      | the cause for sending is gone (recipient already answered, workstream parked, mission updated) | **drop the draft**. Transition thread to `Discarded`/`Done` with a durable obsolescence reference. No send. |
| **Stale-Consolidate**   | multiple drafts/threads must be handled together (same recipient, related topics, queue/ticket overlap) | **reorganize Queue and Tickets first** — merge threads, close superseded items, open consolidating ones — *then* compose a single consolidated draft from the new world. The right output here is **not** another mail but a queue/ticket reorg followed by one mail. |

A reviewer report is the union of findings; routing is determined by the
*highest* class present (`Substantive > Stale-* > Wording`). Among the
Stale sub-classes, `Consolidate > Obsolete > Refresh` (consolidation
implies refresh; obsolescence implies the recipient is no longer the
right target without further analysis).

## 3. Anchor in the Existing Kernel

The kernel in `src/service/core_state_machine.rs` already encodes the relevant
entities and states:

- `CoreEntityType::FounderCommunication`
- `CoreState::{Drafting, DraftReady, Reviewing, Approved, Rejected,
  Sending, Sent, SendFailed, DeliveryRepair, AwaitingAcknowledgement,
  Done, Escalated, Superseded}`
- `CoreEvent::{DraftReply, RequestReview, Approve, Reject, RequireRework,
  Send, ConfirmDelivery, Escalate, Supersede}`
- `CoreEvidenceRefs.{review_audit_key, approved_body_sha256,
  outgoing_body_sha256, approved_recipient_set_sha256,
  outgoing_recipient_set_sha256}`

The reviewed-founder-send adapter in `src/mission/channels.rs:3500
(enforce_reviewed_founder_send_core_transition)` already gates
`Approved → Sending` with body and recipient hashes.

The target state machine for founder communication in
`docs/core_runtime_state_machine.md:323` already lists:

```
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
```

This RFC **does not redesign** the state graph. It identifies the gaps
between this target and the running code, adds the staleness class, and
specifies the witness-of-progress invariants that close the door on
infinite cycles.

## 4. Implementation Gaps Audit

| # | Gap                                                                                       | Evidence                                                                                                       |
|---|-------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------|
| 1 | Body is not durable before `Sending`. CLI-arg only.                                       | `communication_messages` has 2 rows total in production; failed Tag-Proposal body absent.                     |
| 2 | `Sending → SendFailed` transition is not enforced. Provider failures leave no audit row.  | No call site emits `CoreEvent::Fail`/`Escalate` on provider error in the founder-send path.                    |
| 3 | `SendFailed → DeliveryRepair → Sending` retry does not bind to prior `outgoing_body_sha256`. | Retry path is a fresh chat turn; no message_key reference; drafter recomposes.                              |
| 4 | No `Stale` finding class. Reviewer cannot signal "world moved".                          | `service::ReviewRoutingClass = {Approved, RewriteOnly, Substantive}`; no `Stale`.                             |
| 5 | No drift detection between `Approved` and `Sending`.                                     | `enforce_reviewed_founder_send_core_transition` checks body+recipients but not world.                         |
| 6 | No `draft_revisions` lineage table. Cannot prove monotonic substance/world progress.     | No table exists; `communication_artifact_reviews` is point-in-time, not a chain.                              |
| 7 | Generic `enqueue_review_rework` (non-founder) has no attempt counter.                    | Only `FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD = 2` exists, applies only to `founder-communication-rework`.      |
| 8 | Cleanup is not coupled to terminal transitions.                                          | Reaching `Sent`/`Closed` does not trigger reap of child rework self-work, routing rows, queue tasks.          |
| 9 | Process-mining cycle detection is not enforced as a runtime gate.                        | `harness-mining stuck-cases` exists as advisory; no transition rejection on detected cycle.                   |

## 5. Proposed Changes

### 5.1 Body durability before `Sending`

`enforce_reviewed_founder_send_core_transition` must, *before* the provider
call:

1. Insert into `communication_messages` with `direction = 'outbound'`,
   `status = 'draft_pending_send'`, body, recipients, body_sha256, and a new
   column `outbound_revision_id` (see 5.3).
2. Lock the row (advisory lock or CAS on `status`).
3. Only then call the provider.
4. On success: transition `Sending → Sent`, set `status = 'accepted'`,
   record `CoreEvidenceRefs.outgoing_body_sha256` matches the row.
5. On failure: transition `Sending → SendFailed`, set
   `status = 'send_failed'`, persist provider error, do **not** delete the row.

Effect: the body survives provider failure. Retry paths (5.4) reference the
row by `outbound_revision_id`, not by free-form recompose.

### 5.2 Finding classes and routing

Extend the reviewer's `FindingCategory` with the operator-aligned shape:

```rust
enum FindingCategory {
    Rework,            // substantive: content/strategy/evidence wrong
    Rewrite,           // wording: language/tone/format only
    StaleRefresh,      // staleness: new facts arrived, integrate
    StaleObsolete,     // staleness: cause is gone, drop the draft
    StaleConsolidate,  // staleness: must merge with other threads/queue/tickets
}
```

Reviewer prompt asks for explicit classification per finding, with the
required evidence pointer (see §5.5 witness invariants). Routing is the
highest class present, ordered:

```
Substantive       := any finding has Rework
StaleConsolidate  := no Rework, any StaleConsolidate
StaleObsolete     := no Rework, no StaleConsolidate, any StaleObsolete
StaleRefresh      := no Rework, no StaleConsolidate, no StaleObsolete, any StaleRefresh
WordingOnly       := all findings are Rewrite
Approved          := no findings
```

Each routing class has its own worker pathway (§5.4).

### 5.3 Draft revision lineage

New table:

```sql
CREATE TABLE outbound_draft_revisions (
  revision_id TEXT PRIMARY KEY,
  thread_key TEXT NOT NULL,
  parent_revision_id TEXT,                 -- NULL for first
  body_sha256 TEXT NOT NULL,
  body_text TEXT NOT NULL,                 -- full body for retry/diff
  substance_pointer_json TEXT NOT NULL,    -- {mission_state_id, strategy_pointer_ids[], evidence_record_ids[]}
  world_pointer_json TEXT NOT NULL,        -- {thread_last_inbound_message_key, thread_inbound_count, mission_state_id, linked_ticket_state_hashes, active_strategic_directive_ids}
  finding_class TEXT,                      -- 'initial' | 'substantive' | 'wording' | 'stale_refresh' | 'stale_consolidate'
                                           -- (stale_obsolete does not produce a revision; it terminates the chain)
  consolidated_from_revision_ids_json TEXT,-- non-NULL only for stale_consolidate revisions; lists predecessor revisions across threads being merged
  evidence_work_audit_keys_json TEXT,      -- non-NULL only for substantive revisions; lists evidence/verification records that justify the new substance_pointer
  predecessor_review_audit_key TEXT,       -- the review whose finding triggered this revision
  committed_at TEXT NOT NULL,
  superseded_at TEXT,
  superseded_by_revision_id TEXT,
  FOREIGN KEY (parent_revision_id) REFERENCES outbound_draft_revisions(revision_id)
);

CREATE INDEX idx_outbound_draft_revisions_thread_active
  ON outbound_draft_revisions(thread_key, superseded_at);
```

Every `Drafting → DraftReady` writes a new revision row, parented to the
predecessor if any. `body_sha256`, `substance_pointer`, and `world_pointer`
are computed deterministically and recorded.

### 5.4 Class-specific worker pathways

Each finding class triggers a distinct sequence of state transitions. The
*key point* is that not every class returns to `Drafting`. Some terminate
the thread; some go through a Repair lane first.

#### 5.4.1 Wording (in-process, no new task)

```text
Reviewing[WordingOnly] → Drafting → DraftReady → Reviewing
```

Implemented as the existing `RewriteOnly` synthesised in-process turn (see
`service::ReviewRoutingClass::RewriteOnly`). No queue task, no self-work,
no founder-rework spawn. The drafter receives the prior body and the
findings inline, returns a converged body, the new revision is committed.
This is the cheap path; it must remain cheap.

#### 5.4.2 Substantive (real rework with explicit Evidence Work)

```text
Reviewing[Substantive] → ReworkRequired → EvidenceWork → Drafting → DraftReady → Reviewing
```

Substantive findings cannot be fixed by re-wording. The transition kernel
forbids `Reviewing[Substantive] → Drafting` directly; the chain must pass
through `EvidenceWork`. EvidenceWork is satisfied by **at least one** of:

- a new evidence/verification record (`ticket_verifications`,
  `ticket_execution_actions`) committed since the previous review,
- a mission-state transition (`MissionState` entity),
- a strategic-directive update (`strategy_pointer_ids` change),
- explicit operator input recorded in the thread.

The new revision **must** reference the audit keys of the satisfying
evidence (`evidence_work_audit_keys_json`). Without it, the
`Drafting → DraftReady` transition is rejected (WP-Substantive-Evidence,
§5.5).

This is the "echte Rework-Aufgabe" the operator described — not "compose
the same email differently".

#### 5.4.3 Stale-Refresh (integrate new world facts)

```text
Reviewing[StaleRefresh] → Drafting → DraftReady → Reviewing
                          ^
                          new world_pointer captured at draft commit
```

The drafter receives a diff against the prior `world_pointer` and
integrates it. The new revision's `world_pointer` must differ from the
predecessor's on at least one fact axis tied to the finding (WP-Stale,
§5.5). This **does not** count toward the substantive rework counter
(WP-Drift-Reset).

#### 5.4.4 Stale-Obsolete (cause is gone, drop)

```text
Reviewing[StaleObsolete] → Discarded
```

The thread terminates without sending. The transition records:

- the obsolescence trigger (e.g. inbound message key that resolved the
  cause; mission-state ID that parked the workstream),
- a durable reason note,
- cleanup actions per §5.7.

There is no further drafting. Subsequent inbound on the same thread
re-enters at `InboundObserved` with a fresh classification — the
discarded thread is part of context but does not resurrect the prior
draft.

#### 5.4.5 Stale-Consolidate (queue/ticket reorg, then one mail)

This is the most operationally significant case and the one the operator
emphasised: "im Zweifel die Queue und Tickets überarbeiten".

```text
Reviewing[StaleConsolidate] → RepairPlanning → RepairPlanReviewed →
                              ApplyingDeterministicActions →
                              RepairVerification → Restored →
                              Drafting (consolidating revision) →
                              DraftReady → Reviewing
```

The consolidation lane uses the existing `Repair` entity type and event
vocabulary. The reorganization plan is itself a reviewable artifact. Its
deliverables must include:

- thread merges (which threads are joined, which closed),
- queue-task reassignments (which tasks become children of which
  consolidated thread, which are superseded),
- ticket updates (split, merge, close, open),
- the resulting list of consolidated threads that still need outbound.

Only after `Restored`, the worker drafts. The new revision references all
predecessor revisions across the merged threads
(`consolidated_from_revision_ids_json`). The predecessors transition to
`Superseded` with the consolidating revision as
`superseded_by_revision_id`.

#### 5.4.6 Drift gate at `Approved → Sending`

The drift gate runs as a precondition to
`enforce_reviewed_founder_send_core_transition`:

```text
let approved_world_pointer = revision_at_approval.world_pointer
let current_world_pointer  = compute_world_pointer(thread_key, ...)
let drift_kind = classify_drift(approved_world_pointer, current_world_pointer, thread_key)
match drift_kind:
    None:           proceed → Sending
    Refresh:        reject; Approved → Reviewing with finding_class=StaleRefresh
    Obsolete:       reject; Approved → Reviewing with finding_class=StaleObsolete
    Consolidate:    reject; Approved → Reviewing with finding_class=StaleConsolidate
```

`classify_drift` is a pure function. Its rules:

- new inbound on the same thread that addresses the draft's question →
  `Obsolete` candidate (drafter+reviewer confirm by classification);
- new inbound on a parallel thread to the same recipient on a topic that
  shares mission/strategy pointer with this draft → `Consolidate` candidate;
- mission-state or strategy-pointer change touching the draft's
  substance_pointer → `Obsolete` if workstream parked, else `Refresh`;
- new evidence/ticket-state on linked tickets → `Refresh`.

The classifier is conservative: ambiguous drifts default to `Refresh`,
and the reviewer is responsible for re-classifying upward to `Obsolete`
or `Consolidate` if needed.

This drift detection is enforced in the transition kernel — not in agent
prompts.

### 5.5 Witness-of-progress invariants

These invariants are checked by `enforce_core_transition` and rejected
otherwise (recorded in `core_invariant_violations`):

| Invariant | Statement |
|-----------|-----------|
| **WP-Wording**             | A wording-class chain of revisions on the same thread must each have `body_sha256 ≠ predecessor.body_sha256` and `substance_pointer = predecessor.substance_pointer`. |
| **WP-Substantive**         | A `Reviewing → ReworkRequired → EvidenceWork → Drafting → DraftReady` chain requires the new revision's `substance_pointer ≠ predecessor.substance_pointer`. Direct `Reviewing → Drafting` with `finding_class=Rework` is rejected — must pass through EvidenceWork. |
| **WP-Substantive-Evidence**| The new substantive revision's `evidence_work_audit_keys_json` must list at least one evidence/verification/mission-state/strategy/operator-input record committed *between* the prior review and the new revision. Empty list is rejected. |
| **WP-StaleRefresh**        | A `Reviewing → Drafting` with `finding_class=StaleRefresh` requires `world_pointer ≠ predecessor.world_pointer` on at least one axis named in the finding. |
| **WP-StaleObsolete**       | A `* → Discarded` with `finding_class=StaleObsolete` requires a durable obsolescence reference (inbound message key, mission-state ID, or operator note). Empty reason is rejected. |
| **WP-StaleConsolidate**    | A `Reviewing → RepairPlanning` with `finding_class=StaleConsolidate` requires the resulting `Restored → Drafting` revision to list ≥2 predecessor revision IDs in `consolidated_from_revision_ids_json` across distinct thread keys. |
| **WP-Drift-Reset**         | Stale-class revisions (Refresh, Consolidate) do not increment the substantive-rework counter. StaleObsolete terminates the chain entirely. |
| **WP-Cleanup**             | Each terminal transition (`Sent`, `Discarded`, `Superseded`, `Escalated`) must apply the reap actions in §5.7 in the same transaction. Detection of orphaned child rework/queue rows after the transaction is an invariant violation. |

Counter accounting: substantive and wording counters live as derived facts
on the revision chain, not as separate stored state. With WP-Substantive
and WP-Substantive-Evidence in place, a substantive counter of `N` implies
`N` distinct `substance_pointer` values along the chain, each backed by
distinct evidence work — by construction. The state space of meaningful
substance pointers is bounded by the durable mission/strategy/evidence
sets, so the chain is finite.

### 5.6 Bounded-cycle gate

A hard ceiling complements the witness invariants:

- `OUTBOUND_SUBSTANTIVE_REWORK_CEILING = 3` (operator-tunable). Reaching the
  ceiling triggers `Reviewing → Escalated`, queues an operator-visible
  self-work item, and blocks further rework on this thread until operator
  acknowledges.
- `OUTBOUND_WORDING_REWORK_CEILING = 5`. Beyond this, force-route the next
  finding to `Substantive` (escalate) — five wording revisions on the same
  substance is a reviewer convergence failure.
- `OUTBOUND_STALE_REFRESH_CEILING = 4`. Beyond this, escalate — the world is
  moving faster than we draft and the operator must decide.
- `OUTBOUND_STALE_CONSOLIDATE_CEILING = 2`. A second consolidation pass on
  the same surviving thread without intervening Approve/Sent suggests the
  consolidation plan itself is wrong; escalate to operator instead of
  re-attempting.
- StaleObsolete has no ceiling — it is itself a terminal transition.

These ceilings exist as a defense-in-depth guard, not as the primary
loop-freedom argument.

### 5.7 Cleanup as transition

Each terminal transition emits explicit reap actions:

| Transition                   | Reap actions                                                                                          |
|------------------------------|--------------------------------------------------------------------------------------------------------|
| `Sending → Sent`             | mark active revision final; close all `review-rework`/`founder-communication-rework` self-work for thread; set routing rows for related queue tasks to `completed`. |
| `Reviewing → Escalated`      | block all child rework queue tasks; set self-work `state=blocked` with explicit operator note.       |
| `* → Superseded`             | mark all revisions on thread `superseded_at`; close child self-work as superseded.                    |
| `* → Discarded`              | same as Superseded plus persistent reason note.                                                       |

No transition completes without its reap. Leftover orphans are an invariant
violation.

### 5.8 Generic outbound (non-founder)

The generic `enqueue_review_rework` path inherits the same machinery: every
outbound deliverable that requires review goes through
`outbound_draft_revisions` + drift gate + witness invariants + ceilings.
Today's `FOUNDER_REWORK_REQUEUE_BLOCK_THRESHOLD = 2` is replaced by the
class-specific ceilings in 5.6, applied uniformly.

## 6. Loop-Freedom Proof Sketch

Claim: under the rules of §5, no outbound thread can stay in the
non-terminal set `{Drafting, DraftReady, Reviewing, ReworkRequired,
EvidenceWork, RepairPlanning, RepairPlanReviewed,
ApplyingDeterministicActions, RepairVerification, Restored}` without
bound.

1. Every transition out of `Reviewing` carries a `finding_class` (or is
   `Approve` → `Approved`, which leaves the non-terminal set).
2. The class determines the lane:
   - `WordingOnly` → in-process rewrite lane (5.4.1)
   - `Substantive` → EvidenceWork lane (5.4.2)
   - `StaleRefresh` → refresh lane (5.4.3)
   - `StaleObsolete` → terminates with `Discarded` (5.4.4) — leaves the set
   - `StaleConsolidate` → Repair lane (5.4.5)
3. Each lane has a witness-of-progress invariant (§5.5):
   - Wording: `body_sha256 ≠ predecessor`, `substance_pointer = predecessor`
   - Substantive: `substance_pointer ≠ predecessor`, plus non-empty
     `evidence_work_audit_keys_json` proven by referenced records that
     were committed *between* the prior review and this revision
   - StaleRefresh: `world_pointer ≠ predecessor` on a finding-named axis
   - StaleConsolidate: ≥2 distinct predecessor revision IDs from distinct
     thread keys
4. The class-specific ceilings (§5.6) bound each lane:
   - Substantive: ≤3 substantive revisions before `Reviewing → Escalated`
   - Wording: ≤5 wording revisions before forced escalation
   - StaleRefresh: ≤4 refreshes before operator escalation
   - StaleConsolidate: ≤2 consolidation passes before escalation
5. The product space of `(substance_pointer × world_pointer)` is bounded
   by durable runtime state at observation time. Even without ceilings,
   the witness invariants ensure each lane traverses *new* points in this
   bounded space; with ceilings as defense-in-depth, the chain
   terminates in `O(N_substantive · N_refresh · N_consolidate)` steps.
6. Therefore every chain reaches one of the terminal states
   `{Sent, Escalated, Superseded, Discarded}` in bounded steps. ∎

This is a finite-state liveness argument over the *revision graph*, not
over the in-memory turn loop. The turn loop can spin all it wants —
without a revision-chain advance witnessed by §5.5 invariants, no
transition is accepted, and the bounded-cycle gate fires. The
process-mining runtime gate (§7) catches reviewer-side convergence
failures that would otherwise consume budget legitimately while making
no real progress.

## 7. Process-Mining Detection

Static liveness is one half. The other half is detecting actual production
cycles before the ceilings hit:

- `core_state_events` emits one row per accepted transition (entity_type,
  entity_id, from_state, to_state, event, evidence, ts).
- `process_mining` already aggregates these. Add a check: for any
  `FounderCommunication` entity, count distinct `(from_state, to_state)`
  cycle hits in the last `T` hours. If `>K`, raise a
  `harness-mining stuck-case` and surface in `ctox follow-up evaluate`.
- The reviewer prompt is given `process_mining.recent_cycle_hits` for the
  current entity as input. A reviewer that re-issues findings on a previously
  resolved revision is itself a convergence failure, and the reviewer is
  asked to either approve or escalate, not re-issue.

## 8. Migration

Phased, each phase shippable independently:

| Phase | Content                                                                                          | Unblocks                                                          |
|-------|--------------------------------------------------------------------------------------------------|-------------------------------------------------------------------|
| 1     | §5.1 Body durability + Sending → SendFailed transition + retry by `outbound_revision_id`        | The Tag-Proposal class of bug                                     |
| 2     | §5.3 `outbound_draft_revisions` table + chain-writing on every `Drafting → DraftReady`          | Foundation for everything else                                    |
| 3     | §5.2 `Stale` finding class + reviewer prompt change + routing                                   | Reviewer can express staleness                                    |
| 4     | §5.4 Drift gate at `Approved → Sending`                                                          | No more stale outbound after long review queues                   |
| 5     | §5.5 Witness invariants enforced in `enforce_core_transition`                                    | Loop-freedom proof becomes effective                              |
| 6     | §5.6 Bounded-cycle ceilings + escalation                                                         | Defense-in-depth                                                  |
| 7     | §5.7 Cleanup-as-transition                                                                       | Eliminates orphan-row pile-up                                     |
| 8     | §5.8 Generalize to non-founder outbound                                                          | Fixes the no-counter-on-generic-rework gap                        |
| 9     | §7 Process-mining runtime gate                                                                   | Catches reviewer convergence failures in production               |

Each phase ships with scenario tests covering the relevant forbidden states
in `docs/core_runtime_state_machine.md`.

## 9. Test Matrix Additions

To be added to the existing matrix in `docs/core_runtime_state_machine.md`:

| Scenario                                                                                              | Expected result                                                                                  |
|-------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------|
| Provider call fails after `Approved`                                                                  | `Sending → SendFailed`, body row preserved, retry bound to `outbound_revision_id`                |
| Wording-only finding, drafter changes body but not substance                                          | `RewriteOnly` lane accepted; new revision committed; same `substance_pointer`                    |
| Wording-only finding, drafter changes substance                                                       | invariant violation WP-Wording — substance must remain identical on a wording rework             |
| Substantive finding, drafter attempts direct `Reviewing → Drafting`                                   | rejected — must pass through `EvidenceWork`                                                      |
| Substantive rework with empty `evidence_work_audit_keys_json`                                         | invariant violation WP-Substantive-Evidence                                                      |
| Substantive rework citing an evidence record committed *before* the prior review                      | invariant violation WP-Substantive-Evidence (must be between prior review and new revision)      |
| New inbound on same thread between `Approved` and `Sending` that resolves the draft's question        | drift gate fires; `Approved → Reviewing[StaleObsolete]` candidate; reviewer confirms; `Discarded`|
| New inbound on parallel thread to same recipient on related topic                                     | drift gate fires; `Approved → Reviewing[StaleConsolidate]`; Repair lane planned                  |
| StaleRefresh integration: drafter updates `world_pointer` correctly                                   | accepted, substantive counter not incremented (WP-Drift-Reset)                                   |
| StaleObsolete with empty obsolescence reference                                                       | invariant violation WP-StaleObsolete                                                             |
| StaleConsolidate revision with only one predecessor revision_id                                       | invariant violation WP-StaleConsolidate (need ≥2 across distinct threads)                        |
| StaleConsolidate via Repair lane: queue/ticket reorg plan reviewed and applied, then single mail drafted | accepted; predecessor revisions transition to `Superseded`                                    |
| 3 substantive reworks on same thread                                                                  | ceiling triggers `Reviewing → Escalated`; operator self-work created                             |
| 5 wording revisions on same substance                                                                 | ceiling triggers force-escalation to substantive review                                          |
| Reviewer re-issues identical finding on already-superseded revision                                   | process-mining detects cycle; reviewer asked to escalate, not re-issue                           |
| Outbound non-founder thread without revision row                                                      | `Approved → Sending` rejected (unified gate; §5.8)                                               |
| Cleanup-on-Sent: child review-rework self-work still open after transition                            | invariant violation WP-Cleanup; transaction rolled back                                          |
| Discarded thread receives new inbound later                                                           | enters `InboundObserved` fresh; the discarded revision does not resurrect                        |

## 10. Out of Scope

- Reviewer LLM behavior optimization (different RFC).
- Multi-channel outbound (Teams/Slack/Jami) — same machinery applies but
  channel-specific persistence formats are out of this RFC's scope.
- Inbound classification (covered by existing target).
- Knowledge capture after escalation (existing target).

## 11. Open Questions

1. `substance_pointer` content: should `evidence_record_ids` reference
   verification rows, ticket knowledge entries, or both? Probably both,
   with a stable hash over the union.
2. World-pointer scope for `Stale` detection: do operator TUI prompts on
   the same thread count as world-drift? Default: yes — operator input is
   first-class world state.
3. Reviewer-side: should the reviewer be required to attach
   `evidence_record_ids` to a Stale finding (i.e. point to the new inbound
   or new mission-state), or is the diff sufficient? Default: reviewer
   must point — opaque "things changed" is not acceptable.
4. `classify_drift` (§5.4.6) ambiguity: a parallel-thread inbound to the
   same recipient *might* warrant Consolidate or might be unrelated. How
   does the kernel decide vs. defer to reviewer? Default: kernel emits
   `Refresh` with a `consolidate_candidate=true` flag; reviewer
   re-classifies upward if the topics are tied.
5. StaleConsolidate Repair plan reviewability: is the queue/ticket reorg
   plan itself a `FounderCommunication`-class artifact (because the
   downstream mail is) or a `Repair`-class artifact? Default: `Repair`,
   reviewed under the existing `RepairPlanning → RepairPlanReviewed` gate.
6. Backwards compatibility for existing in-flight threads at migration:
   do they retroactively get a synthetic initial revision, or are they
   grandfathered into a legacy code path until they reach a terminal
   state? Default: synthetic initial revision with
   `parent_revision_id=NULL`, `finding_class='migration_synthetic'`,
   `world_pointer` snapshotted at migration time.
