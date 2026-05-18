---
name: queue-cleanup
description: Use when the CTOX service queue is under pressure, duplicate scheduled work is piling up, pending prompts are growing too fast, or CTOX risks blocking itself behind repeated queue or scheduler work. This skill investigates queue pressure, pauses flooding schedules when needed, deduplicates future work, and restores a stable queue before normal execution resumes.
cluster: mission_orchestration
---

# Queue Cleanup

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.

## Core Spawn Intervention Contract

When this skill is invoked because the Core Spawn Gate rejected a spawn or detected a loop, it must act as a bounded intervention and must not create new durable work.

Allowed intervention effects:

- block or cancel the rejected child queue/self-work item
- consolidate useful child evidence into the existing parent work item
- release or requeue the existing parent item
- mark redundant work terminal with a clear reason

Do not run commands that create new queue tasks, ticket self-work, schedules, plans, or published spills while handling a Core Spawn Gate intervention. In particular, do not use `ctox queue spill --publish`, `ctox ticket self-work-put`, `ctox schedule ensure`, or `ctox plan ingest` for the intervention path. If new work seems necessary, block the current item with the reason and let the parent work item or operator decide.


Use this skill when CTOX is at risk of self-blocking because too much work is piling up in the queue or inbound routing path.

Do not use this skill for normal task planning. Use it only when queue pressure, repeated scheduler emissions, duplicate follow-up work, or self-induced backlog is the problem.

If the issue is about normal durable work decomposition, use `queue-orchestrator` instead.

## Core Goal

Stabilize the queue first.

Queue cleanup must preserve durable CTOX knowledge and work state. Queue text, spilled prompts, or workspace notes do not replace continuity, ticket state, verification state, or ticket knowledge.

The next work step should:

- identify why the queue is growing
- stop further flooding if needed
- preserve the minimum safe work
- avoid destroying valid operator work

## Tools

Use these tools directly:

- `ctox status`
- `ctox schedule list`
- `ctox schedule pause --task-id <id>`
- `ctox schedule resume --task-id <id>`
- `ctox schedule remove --task-id <id>`
- `ctox queue list`
- `ctox queue show --message-key <key>`
- `ctox queue spill-candidates [--limit <n>]`
- `ctox queue spills [--state <spilled|restored>] [--limit <n>]`
- `ctox queue block --message-key <key> --reason <text>`
- `ctox queue cancel --message-key <key> --reason <text>`
- `ctox queue release --message-key <key> --note <text>`
- `ctox queue spill --message-key <key> [--ticket-system <name>] [--reason <text>] [--publish]`
- `ctox queue restore --message-key <key> [--priority <urgent|high|normal|low>] [--note <text>]`
- `ctox ticket self-work-show --work-id <id>`

Harness signals to consult before acting:

- `ctox harness-mining stuck-cases --min-attempts 5 --limit 50`
  Returns entities whose preventive layer kept rejecting them. Read `cases[].entity_id`,
  `rejected_attempts`, `last_attempted_to_state`, and `dominant_violation_codes_json`.
  An entity with ≥5 rejected attempts is a retry-loop suspect — block it before spilling
  so the loop stops, do not just spill.
- `ctox harness-mining sojourn --limit 50`
  Returns per-state holding-time percentiles. Read `states[].state` together with
  `p95_seconds` and `p99_seconds`. A queue-related state with extreme p95 is a bottleneck,
  not a flooding source — pause/throttle the producer rather than draining the consumer.

If needed for diagnosis, inspect SQLite state directly:

- `sqlite3 runtime/ctox.sqlite3`
- `sqlite3 runtime/ctox.sqlite3`

## Workflow

1. Confirm queue pressure.
   Use `ctox status` and identify whether `pending_count` is rising, stuck, or dominated by one source.
2. Surface retry-loops before any other cleanup.
   Run `ctox harness-mining stuck-cases --min-attempts 5 --limit 50`. For each returned
   `entity_id`, derive its `message_key` and `ctox queue block --message-key <key>
   --reason "harness-mining: <N> rejected attempts at <to_state>"`. This stops the loop
   before you risk spilling work that will just re-flood after restore.
3. Identify the flooding source.
   Check `ctox schedule list` and `ctox queue list` for repeated task sources, duplicated prompts, or stuck follow-up work.
   If `harness-mining sojourn` reports a queue state with p95 > 600s, treat the producer
   for that state as a flooding suspect.
4. Identify explicit spill candidates.
   Use `ctox queue spill-candidates` to find lower-risk or already blocked work that can leave the hot queue without being lost.
5. Contain the producer.
   If one schedule or queue source is clearly flooding the system, pause or block that source before doing anything else.
6. Preserve valid work.
   Do not cancel unrelated operator work just because the queue is busy.
7. Minimize duplicates.
   Prefer blocking, pausing, or cancelling only the repeated or clearly redundant work.
8. Spill durable work into the internal ticket system if the queue cannot safely hold all valid work at once.
   Use `ctox queue spill` for work that must stay visible and durable but should temporarily leave the hot queue. Restore it with `ctox queue restore` when capacity returns.
   Use `ctox queue spills` to review what is currently parked outside the hot queue.
9. Restore a safe backlog.
   The queue should return to a size that CTOX can drain safely.
10. Report what changed.
    State what was paused, blocked, cancelled, preserved, and what still needs follow-up.
    Include a `Harness signals` line listing any blocked retry-loops with their
    `dominant_violation_codes_json`, so the operator sees what was contained.

## Guardrails

- Do not wipe the queue blindly.
- Do not cancel user work unless it is clearly duplicate or harmful.
- Pause the flooding producer before trimming downstream backlog.
- Keep one clear surviving path for valid work.
- If you cannot prove a task is duplicate, block or reprioritize it instead of deleting it.

## Operator Feedback

Answer in this order:

1. `Status`
2. `State`
3. `Scope`
4. `Autonomous Actions`
5. `Escalation`
6. `Current Findings`
7. `Next Step`

`State` must be one of:

- `proposed`
- `prepared`
- `executed`
- `blocked`

Inside these headings, spell out:

- queue pressure and backlog under `Scope`
- paused, blocked, cancelled, or deduplicated work under `Autonomous Actions`
- anything still unsafe or requiring owner review under `Escalation`
- active flooding sources and surviving valid work under `Current Findings`
