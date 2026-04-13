---
name: queue-orchestrator
description: Use when work should be added to, inspected in, reprioritized within, or otherwise managed through the explicit CTOX queue that feeds the normal inbound routing path.
metadata:
  short-description: Manage the explicit CTOX execution queue
---

# Queue Orchestrator

Use this skill when Codex should work with durable queued tasks instead of keeping a hidden todo list.

## Core Rule

The queue is explicit shared state. If you want future work to survive beyond the current turn, use `ctox queue ...`.

Do not assume an external wrapper will decide the next task for you.

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

## Operating Pattern

1. Read the queue if ordering or existing work matters.
2. Add follow-up work explicitly with `ctox queue add`.
3. If a task should be decomposed first, use `ctox plan draft` and then enqueue only the concrete slices that should really execute.
4. Prefer one coherent queue task per execution slice.
5. Use `--parent-message-key` when a follow-up task clearly descends from an earlier queue item.

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
