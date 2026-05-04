---
name: queue-orchestrator
description: Use when work should be added to, inspected in, reprioritized within, or otherwise managed through the explicit CTOX queue that feeds the normal inbound routing path.
metadata:
  short-description: Manage the explicit CTOX execution queue
cluster: mission_orchestration
---

# Queue Orchestrator

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill when Codex should work with durable queued tasks instead of keeping a hidden todo list.

## Core Rule

The queue is explicit shared state. If you want future work to survive beyond the current turn, use `ctox queue ...`.

Do not assume an external process will decide the next task for you.

Queue state is not the full knowledge plane. Durable knowledge lives in SQLite-backed continuity, ticket state, verification state, communication records, and ticket knowledge. Standalone markdown files or workspace notes do not become durable knowledge just because a queue item references them.

## Commands

### Inspect Queue State

```sh
ctox queue list [--status "<status>"]... [--limit "<n>"]
ctox queue show --message-key "<message_key>"
```

Use this before reordering or adding follow-up work when the current queue state matters.

### Add A New Queue Task

```sh
ctox queue add --title "<short label>" --prompt "<full task prompt>" [--thread-key "<thread>"] [--skill "<skill>"] [--priority "<urgent|high|normal|low>"] [--parent-message-key "<message_key>"]
```

Behavior:

- Creates a visible inbound queue task.
- The queue task enters the same routing substrate used by other inbound work.
- Nothing is hidden from Codex; later service execution leases the same queue item.

### Edit Or Reprioritize An Existing Queue Task

```sh
ctox queue edit --message-key "<message_key>" [--title "<label>"] [--prompt "<text>"] [--thread-key "<thread>"] [--skill "<skill>"] [--clear-skill] [--priority "<urgent|high|normal|low>"]
ctox queue reprioritize --message-key "<message_key>" --priority "<urgent|high|normal|low>"
```

### Mark Queue Outcome

```sh
ctox queue block --message-key "<message_key>" --reason "<why>"
ctox queue release --message-key "<message_key>" [--priority "<urgent|high|normal|low>"] [--note "<text>"] [--clear-note]
ctox queue fail --message-key "<message_key>" --reason "<why>"
ctox queue complete --message-key "<message_key>" [--note "<text>"]
ctox queue cancel --message-key "<message_key>" [--reason "<why>"]
```

### Read Harness Signals Before Reordering

```sh
ctox harness-mining stuck-cases --min-attempts 5 --limit 20
ctox harness-mining sojourn --entity-type queue --limit 20
ctox harness-mining variants --entity-type queue --limit 10
```

What to look at:

- `cases[].entity_id` + `rejected_attempts` from `stuck-cases` — if a queue item has
  ≥5 rejected preventive proofs, treat it as a retry-loop. Do not `release` it; either
  `block` with `--reason "harness-mining: <N> retries"` or `fail` it.
- `states[].p95_seconds` from `sojourn` — a queue state with extreme p95 indicates a
  bottleneck downstream of that state. Reordering queue items will not fix it; surface
  it as escalation instead.
- `pareto.variants_for_80pct` from `variants` — if 80% of queue traces collapse into
  just 1–2 variants, the queue is doing one repetitive thing. New queue work that
  fits an existing variant is likely redundant; prefer `--parent-message-key` linking
  over creating a fresh queue item.

## Operating Pattern

1. Read the queue if ordering or existing work matters. If you are about to release,
   reprioritize, or reorder live items, first run `ctox harness-mining stuck-cases`
   to make sure none of the candidates are hot retry-loops.
2. Add follow-up work explicitly with `ctox queue add`.
3. If a task should be decomposed first, use `ctox plan draft` and then enqueue only the concrete bounded work steps that should really execute.
4. Prefer one coherent queue task per bounded work step.
5. Use `--parent-message-key` when a follow-up task clearly descends from an earlier queue item.
6. When queueing ticket-bearing or owner-visible work, first inspect whether the ticket and knowledge subsystems are actually populated. If `ticket_items`, `ticket_cases`, source skills, or knowledge domains are still absent, phrase the queue task as onboarding / correction work rather than normal mature execution.
7. Do not use queue text as a substitute for durable knowledge persistence. If the mission understanding needs to survive, ensure it also lands in SQLite-backed continuity or ticket knowledge.

## Important Boundaries

- The queue tool changes explicit queue state only.
- It does not replace the Codex execution loop.
- It does not inject hidden planning logic.
- It does not send owner communication itself.

## Do Not

- Do not create speculative queue tasks for work that is not actually needed.
- Do not leave blockers only in prose if the queue item should stop moving.
- Do not assume leased work is invisible; it is still the same queue item moving through the normal inbound path.

## Contracts

Read `references/queue-contracts.md` before using this skill for real work.
