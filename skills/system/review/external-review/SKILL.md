---
name: external-review
description: Run an external, read-only verification pass for a CTOX task result by gathering mission, ticket, communication, runtime, and live-surface evidence directly from tools instead of relying on executor-provided context.
cluster: review
---

# External Review

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill for a standalone review run.

Treat the review assignment as target metadata only.
Gather everything else yourself.

Only CTOX runtime store, live surfaces, repo state, and direct read-only verification count as durable evidence. Standalone markdown artifacts or workspace notes do not count as knowledge unless their facts are also reflected in the CTOX runtime store or directly verified live.

Recent meeting outcomes are time-sensitive runtime evidence. For communication reviews and artifact reviews, inspect the latest relevant meeting summaries before verdict. Prefer same-thread meetings first, then recent cross-thread meeting summaries that mention the reviewed system, recipient, deliverable, project, or artifact. Treat the last 7 days as high-priority context, 30-day-old meetings as supporting context, and older meeting notes as stale unless reinforced by current runtime state.

## Core Contract

The review run:

- uses read-only inspection only
- rebuilds its own understanding from the runtime store and live surfaces
- evaluates the reviewed task result against mission state, done gates, claims, and public-surface quality
- returns a verdict, failed gates, open items, evidence, and when needed a handoff for another review run
- for founder/owner outbound email drafts, treats “do not send yet; wait for specific founder input” as a terminal review result: return `VERDICT: FAIL`, begin `SUMMARY:` with `NO-SEND:`, state the wait condition, and put `none` under `OPEN_ITEMS` unless real work is missing

## Primary Sources

1. Runtime store: `runtime/ctox.sqlite3`
2. Workspace under review
3. Live public/runtime URLs
4. Ticket/self-work state
5. Relevant communication facts
6. Service/runtime/log state
7. Active strategic directives (Vision and Mission) stored in CTOX runtime state

## Suggested Workflow

1. Read the review assignment carefully.
2. Resolve the target conversation/thread in the runtime store.
3. Discover:
   - active vision
   - active mission
   - done gate
   - latest claimed task result
   - current blockers
   - active/open related work
4. Inspect related ticket/self-work state.
5. Inspect recent relevant meeting outcomes, especially before owner/founder communication or artifact-readiness verdicts.
6. Inspect relevant communication facts if the work is owner-visible.
7. Inspect the live surface and critical routes.
8. Inspect the relevant files/runtime/logs needed to settle the claims.
9. Decide PASS / FAIL / PARTIAL from evidence.
10. If the ticket+knowledge subsystem is not operationalized end-to-end, treat that as part of the mission-state finding rather than assuming missing knowledge does not matter.

## Canonical Read-Only Commands

Use the local CTOX CLI first where it gives a structured answer.

### Continuity and mission state

```bash
ctox strategy show --conversation-id <conversation-id> --thread-key <thread-key>
```

```bash
ctox continuity-show runtime/ctox.sqlite3 <conversation-id> focus
ctox continuity-show runtime/ctox.sqlite3 <conversation-id> anchors
ctox continuity-show runtime/ctox.sqlite3 <conversation-id> narrative
```

```bash
sqlite3 runtime/ctox.sqlite3 "
SELECT conversation_id, mission, mission_status, blocker, done_gate AS finish_rule, next_slice AS next_step, is_open
FROM mission_states
WHERE conversation_id = <conversation-id>
ORDER BY updated_at DESC;
"
```

### Latest conversation activity

```bash
sqlite3 runtime/ctox.sqlite3 "
SELECT message_id, role, created_at, substr(body,1,400)
FROM messages
WHERE conversation_id = <conversation-id>
ORDER BY message_id DESC
LIMIT 12;
"
```

### Queue and self-work

```bash
ctox queue list
ctox ticket self-work-list --limit 20
ctox ticket cases --limit 20
```

```bash
sqlite3 runtime/ctox.sqlite3 "
SELECT message_key, source_label, status, thread_key, priority, preview
FROM queue_messages
ORDER BY created_at DESC
LIMIT 20;
"
```

### Communication facts

```bash
sqlite3 runtime/ctox.sqlite3 "
SELECT channel, direction, sender_address, subject, substr(preview,1,220), created_at
FROM communication_messages
WHERE thread_key = '<thread-key>'
ORDER BY created_at DESC
LIMIT 12;
"
```

### Recent meeting outcomes

```bash
sqlite3 runtime/ctox.sqlite3 "
SELECT observed_at, thread_key, subject, substr(body_text,1,1200)
FROM communication_messages
WHERE channel = 'meeting'
  AND (
    body_text LIKE '%Meeting Summary%'
    OR subject LIKE '%meeting%'
    OR subject LIKE '%Meeting%'
  )
ORDER BY observed_at DESC
LIMIT 12;
"
```

For same-thread review context, add `AND thread_key = '<thread-key>'` first. If same-thread results are empty, search recent meeting summaries globally and keep only entries that mention the artifact, reviewed system, recipient, project, or deliverable. Do not let an old meeting override a newer ticket, continuity, communication, or verification record.

### Live/public verification

```bash
curl -I <public-url>
curl -sS <public-url>
curl -i <critical-route>
```

Use a browser for owner-visible or public surfaces whenever possible.

### Harness conformance and constraint coverage

```bash
ctox harness-mining conformance --window 2000 --fitness-threshold 0.95
ctox harness-mining multiperspective --limit 30
```

What to read for the verdict:

- `conformance.fitness_ok`: if `false`, the harness is acting outside its
  declared spec. This alone is sufficient to set `MISSION_STATE: UNHEALTHY`
  even if the public surface looks fine. Quote the failing buckets in
  `EVIDENCE`.
- `conformance.trigger.failing_buckets[]`: each row names an out-of-catalog
  transition (e.g. `ticket: queued → closed` skipping `verified`). Treat
  this as a `FAILED_GATES` entry — it is a Spec-vs-Reality drift the
  reviewed task result has either caused or inherited.
- `multiperspective.evidence_presence`: per evidence-key presence ratio
  across recent proofs. A `review_audit_key` presence ratio < 1.0 on any
  protected entity type is a critical gate failure for owner-visible work.
- `multiperspective.constraint_coverage[].acceptance_ratio`: rejected →
  accepted ratio per (entity_type, lane, from→to). Use this to spot review
  bottlenecks — a transition with high rejected count and low acceptance
  ratio is a procedural break, not a one-off bug.

## Finding categories

Every concrete finding the review run produces must be tagged with one of:

- **`rewrite`** — pure body / subject / wording correction; the agent can fix it by editing the prior outbound body without changing any durable state. Examples:
  - internal-vocabulary leak in prose ("TUI", "queue", "sqlite", "reviewed founder send Pfad", etc.)
  - salutation wrong for audience register (formal Sie when collegial expected, or vice-versa)
  - tone or register slip
  - typos, grammar, capitalisation
  - politeness formula missing or wrong
  - body too long / too short for purpose
  - subject suboptimal
  - line breaks / formatting

- **`rework`** — substantive change requiring durable state mutation, fresh research, or new structural artefacts. Examples:
  - mismatch between body claim and durable state (strategic_directives, mission_states, communication_messages, communication_routing_state)
  - missing durable backing record (planned_goal, planned_step, ticket, founder_reply_review approval)
  - missing recipient anchor for proactive outbound (no inbound reply, no clear new purpose, recent overlapping mail to same recipients)
  - body asserts something not supported by evidence the reviewer was able to gather
  - audience misrouted (recipient classification doesn't match content)
  - mission contract incomplete: the finish rule or next step is missing while the task claims completion-readiness

If a finding could plausibly be either, default to `rework`.

The agent's machine-readable output for the verdict must include each finding with `id`, `category` (one of `rewrite` / `rework`), `evidence`, and `corrective_action`. The dispatcher uses these tags structurally — there is no string scraping in core.

Emit one entry per finding under the `CATEGORIZED_FINDINGS:` block as a single line of pipe-delimited `key: value` pairs in the order `id | category | evidence | corrective_action`. Use `- none` if the run produced no concrete findings.

## Public Launch Failure Conditions

Return FAIL when any of these are true:

- active vision or active mission is missing for strategic or owner-visible work
- internal instruction text is visible
- planning or operator text is visible
- admin or backoffice surfaces leak into the buyer flow
- critical route or dependent API is broken
- the page is technically up but commercially not credible
- the layout, hierarchy, or copy is visibly not launch-worthy

## Stateful Product Failure Conditions

When the reviewed task result claims a product with UI, database-backed workflow, or AI/agent automation, apply the `stateful-product-from-scratch` review contract. Return FAIL when any of these are true:

- the central object, states, gates, or transitions are not modeled durably
- UI state is backed mainly by frontend arrays or demo fixtures
- UI writes bypass repository/service/backend mutation paths
- drag/drop or status changes are not backend-validated
- an AI/agent step exists only as prompt copy and has no CLI/API/tool contract
- progress, logs, chat/messages, blockers, or transition results are not persisted
- a progress bar or status label exists without a durable `TransitionRun` or equivalent
- failure/blocker states are not visible to the user and stored
- the claimed main flow lacks browser QA or mutation smoke evidence

Default classification for these findings is `rework`, not `rewrite`.

## System Integration Failure Conditions

Return FAIL when the reviewed task result touches an external system and any of these are true:

- (a) A new `ticket_source_control` row (a Kanban source) exists without an active `source-skill-binding` and without an open `system-onboarding` self-work item.
- (b) A non-Kanban system (CRM, API, platform, codebase, database) is referenced in the mission and live-touched, but `ctox ticket knowledge-list --system "<s>"` returns 0 entries.
- (c) Live work against the system happened (outbound to external contacts, data mutation, connected-app or permission setup) without an open onboarding self-work item.
- (d) The reviewed task result operationally touched a new system but produced no `ticket_knowledge_loads` and no new `ticket_knowledge_entries`.
- (e) The reviewed task claims CTOX learned a reusable procedure, but no source skill, skillbook, runbook, or runbook item was created or updated.

Reviewer checks for these conditions:

```bash
ctox ticket sources
ctox ticket source-skills
ctox ticket knowledge-list --system "<system>"
ctox ticket self-work-list --system "<system>" --state open
```

## Review Handoff Rule

Normal review compaction is disabled.

If the review grows large enough that another review run should continue, stop and return:

- `VERDICT: PARTIAL`
- decisive facts gathered so far
- remaining checks
- best next verification targets

The handoff must be sufficient for another reviewer to continue without the original run.

## Output Contract

Return exactly:

- `VERDICT: PASS|FAIL|PARTIAL`
- `MISSION_STATE: HEALTHY|UNHEALTHY|UNCLEAR`
- `SUMMARY: ...`
- `FAILED_GATES:`
- `FINDINGS:`
- `CATEGORIZED_FINDINGS:`
- `OPEN_ITEMS:`
- `EVIDENCE:`
- `HANDOFF:`
