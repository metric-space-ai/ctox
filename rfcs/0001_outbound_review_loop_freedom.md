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
exactly one of three orthogonal critique classes per finding:

| Class            | Reviewer means                                 | Worker action                                                                                            |
|------------------|------------------------------------------------|----------------------------------------------------------------------------------------------------------|
| **Substantive**  | content / strategy / evidence is wrong         | re-draft with **new substance**: change `substance_pointer` (mission state ref, strategy refs, evidence) |
| **Wording**      | content is fine, language/tone/format is off   | rewrite-only convergence: same `substance_pointer`, new `body_hash`                                      |
| **Staleness**    | the world moved while the draft sat            | refresh: integrate the new world facts, fresh `world_pointer`, body may or may not change                |

A finding is one of these three classes. A reviewer report is the union of
findings; routing is determined by the *highest* class present
(`Substantive > Staleness > Wording`).

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

### 5.2 New finding class: `Stale`

Extend the reviewer's `FindingCategory`:

```rust
enum FindingCategory { Rework, Rewrite, Stale }
```

Reviewer prompt asks for explicit classification. Routing is the max class
present:

```
Substantive  := any finding has category=Rework
Stale        := no Rework, but ≥1 Stale
WordingOnly  := all findings are Rewrite
Approved     := no findings
```

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
  finding_class TEXT,                      -- the class that produced this revision: 'initial' | 'substantive' | 'wording' | 'stale'
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

### 5.4 Drift gate at `Approved → Sending`

Precondition added to
`enforce_reviewed_founder_send_core_transition`:

```text
let approved_world_pointer = revision_at_approval.world_pointer
let current_world_pointer = compute_world_pointer(thread_key, ...)
if drift(approved_world_pointer, current_world_pointer) is non-empty:
    reject Approved → Sending
    emit transition Approved → Reviewing with finding_class='stale'
                                            staleness_diff_json=<diff>
    do not call provider
```

`drift` is a pure function over the world_pointer fields. New inbound on the
same thread, mission state change, or any linked-ticket state change since
approval are non-empty drifts. Unrelated thread updates are not.

This is the only mechanism by which a stale draft is prevented from going
out, and it is enforced in the transition kernel — not in agent prompts.

### 5.5 Witness-of-progress invariants

These invariants are checked by `enforce_core_transition` and rejected
otherwise (recorded in `core_invariant_violations`):

| Invariant | Statement |
|-----------|-----------|
| **WP-Substantive** | A `Reviewing → Drafting` transition with `finding_class=Rework` requires the next `Drafting → DraftReady` to commit a revision whose `substance_pointer ≠ predecessor.substance_pointer`. |
| **WP-Wording** | A `Reviewing → Drafting` with `finding_class=Rewrite` requires `body_sha256 ≠ predecessor.body_sha256` and `substance_pointer = predecessor.substance_pointer`. |
| **WP-Stale** | A `Reviewing → Drafting` with `finding_class=Stale` requires `world_pointer ≠ predecessor.world_pointer` (refresh integrated). |
| **WP-Drift-Reset** | `Stale` revisions do not increment the substantive-rework counter. |

The counters live in the revision chain (count of non-`Stale` parents per
thread), not in a separate state. With WP-Substantive in place, a substantive
counter of `N` implies `N` distinct `substance_pointer` values along the
chain. The state space of meaningful substance pointers is bounded by the
durable mission/strategy/evidence sets, so the chain is finite by
construction.

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

Claim: under the rules of §5, no outbound thread can stay in
`{Drafting, DraftReady, Reviewing}` without bound.

1. Every `Reviewing → Drafting` carries a `finding_class`.
2. By WP-Substantive, WP-Wording, WP-Stale, the new revision differs from its
   parent on at least one of `(substance_pointer, body_sha256, world_pointer)`
   — and on an axis specifically tied to the finding class.
3. The product space of `(substance_pointer × world_pointer)` is bounded by
   the durable runtime state at observation time. `body_sha256` is an
   unbounded set in principle, but the wording ceiling (5.6) bounds wording
   chains.
4. Substantive ceiling (5.6) terminates the substantive chain at `N=3`.
5. Stale ceiling (5.6) terminates the stale chain at `N=4`.
6. Therefore every chain reaches one of `{Sent, Escalated, Superseded,
   Discarded}` in bounded steps. ∎

This is a finite-state liveness argument over the *revision graph*, not over
the in-memory turn loop. The turn loop can spin all it wants — without a
revision-chain advance, no transition is accepted, and the bounded-cycle
gate fires.

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

| Scenario                                                                | Expected result                                                       |
|-------------------------------------------------------------------------|-----------------------------------------------------------------------|
| Provider call fails after `Approved`                                    | `Sending → SendFailed`, body row preserved, retry path bound to revision |
| Reviewer issues `Substantive` finding without changed substance pointer on next draft | invariant violation WP-Substantive, transition rejected |
| New inbound on thread between `Approved` and `Sending`                  | drift gate fires, `Approved → Reviewing` with `Stale` class, no send |
| 4 substantive reworks on same thread                                    | ceiling triggers `Reviewing → Escalated`                              |
| Reviewer re-issues identical finding on already-superseded revision     | process-mining detects cycle, reviewer is asked to escalate           |
| Outbound non-founder thread without revision row                        | `Approved → Sending` rejected (unified gate)                          |
| Cleanup-on-Sent: child review-rework self-work still open               | cleanup invariant violation                                           |

## 10. Out of Scope

- Reviewer LLM behavior optimization (different RFC).
- Multi-channel outbound (Teams/Slack/Jami) — same machinery applies but
  channel-specific persistence formats are out of this RFC's scope.
- Inbound classification (covered by existing target).
- Knowledge capture after escalation (existing target).

## 11. Open Questions

1. `substance_pointer` content: should `evidence_record_ids` reference
   verification rows, ticket knowledge entries, or both? Probably both, with
   a stable hash over the union.
2. World-pointer scope for `Stale` detection: do operator TUI prompts on the
   same thread count as world-drift? Default: yes — operator input is
   first-class world state.
3. Reviewer-side: should the reviewer be required to attach
   `evidence_record_ids` to a `Stale` finding (i.e. point to the new
   inbound or new mission-state), or is the diff sufficient? Default:
   reviewer must point — opaque "things changed" is not acceptable.
