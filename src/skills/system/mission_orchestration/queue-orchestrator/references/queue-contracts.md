# CTOX Queue Contracts

This document defines the explicit queue substrate that Codex can inspect and modify through tools.

## Queue Contract

- A queue task is an inbound work item stored in the shared channel database.
- Queue tasks must remain compact and self-contained.
- Queue tasks are visible state, not hidden internal planning notes.

## Ownership Contract

- Codex may inspect, add, edit, reprioritize, block, fail, complete, or cancel queue tasks through `ctox queue ...`.
- The queue does not create a second execution engine; it only feeds the existing inbound routing path.

## Priority Contract

- Supported priorities are `urgent`, `high`, `normal`, and `low`.
- Priority affects the queue order only; it does not change task scope.
- Reprioritizing a task should not silently rewrite its actual requirements.

## Status Contract

- Stable queue statuses are `pending`, `leased`, `blocked`, `failed`, `handled`, and `cancelled`.
- `pending` means eligible for future lease into the normal execution loop.
- `leased` means the service already pulled the queue item into execution.
- `blocked` means further work should stop until the blocker is cleared.
- `failed` means the queue item attempted execution and still needs an explicit next decision.
- `handled` means the current queue item is complete for its intended work step.
- `cancelled` means the queue item should no longer run.

## Follow-up Contract

- If a turn ends with real remaining work, Codex should create or edit the next queue task explicitly.
- Follow-up creation is an explicit tool action, not an automatic side effect of evaluation.

## Planning Contract

- Use `ctox plan draft` when a task first needs decomposition.
- Only enqueue the concrete bounded work steps that should really enter the queue.
- Do not dump raw planning traces into queue prompts.
