---
name: queue-cleanup
description: Use when the CTOX service queue is under pressure, duplicate scheduled work is piling up, pending prompts are growing too fast, or CTOX risks blocking itself behind repeated queue or scheduler work. This skill investigates queue pressure, pauses flooding schedules when needed, deduplicates future work, and restores a stable queue before normal execution resumes.
---

# Queue Cleanup

Use this skill when CTOX is at risk of self-blocking because too much work is piling up in the queue or inbound routing path.

Do not use this skill for normal task planning. Use it only when queue pressure, repeated scheduler emissions, duplicate follow-up work, or self-induced backlog is the problem.

If the issue is about normal durable work decomposition, use `queue-orchestrator` instead.

## Core Goal

Stabilize the queue first.

The next task slice should:

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

If needed for diagnosis, inspect SQLite state directly:

- `sqlite3 runtime/cto_agent.db`
- `sqlite3 runtime/ctox_lcm.db`

## Workflow

1. Confirm queue pressure.
   Use `ctox status` and identify whether `pending_count` is rising, stuck, or dominated by one source.
2. Identify the flooding source.
   Check `ctox schedule list` and `ctox queue list` for repeated task sources, duplicated prompts, or stuck follow-up work.
3. Identify explicit spill candidates.
   Use `ctox queue spill-candidates` to find lower-risk or already blocked work that can leave the hot queue without being lost.
4. Contain the producer.
   If one schedule or queue source is clearly flooding the system, pause or block that source before doing anything else.
5. Preserve valid work.
   Do not cancel unrelated operator work just because the queue is busy.
6. Minimize duplicates.
   Prefer blocking, pausing, or cancelling only the repeated or clearly redundant work.
7. Spill durable work into the internal ticket system if the queue cannot safely hold all valid work at once.
   Use `ctox queue spill` for work that must stay visible and durable but should temporarily leave the hot queue. Restore it with `ctox queue restore` when capacity returns.
   Use `ctox queue spills` to review what is currently parked outside the hot queue.
8. Restore a safe backlog.
   The queue should return to a size that CTOX can drain safely.
9. Report what changed.
   State what was paused, blocked, cancelled, preserved, and what still needs follow-up.

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
