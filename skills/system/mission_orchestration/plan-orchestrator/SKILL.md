---
name: plan-orchestrator
description: Use when a task first needs explicit decomposition or revalidation before execution, especially when Codex should produce a compact plan artifact without leaking planning traces into the later execution context.
metadata:
  short-description: Draft compact CTOX plans without context bleed
cluster: mission_orchestration
---

# Plan Orchestrator

## CTOX Runtime Contract

- Task spawning is allowed only for real bounded work steps that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct bounded work step with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill when the user's request is too large, too staged, or too interruptible for one direct Codex turn.

## When To Use

- The task obviously has multiple dependent steps.
- The work should be decomposed before execution or before explicit queue entry.
- The request may block on missing user input and should surface that block explicitly.
- The owner wants durable progress tracking rather than one transient answer.

## Core Rule

Do not improvise your own hidden todo list when the task should first be decomposed. Use the `ctox plan` commands so the plan becomes an explicit compact artifact.

Planning must not dump internal reasoning into the durable conversation context.
Use `ctox plan draft` first when you need a temporary plan artifact. Only persist with `ctox plan ingest` when you intentionally want explicit plan state beyond the current turn.

Plan text is not a knowledge plane by itself. Durable mission knowledge must end up in CTOX continuity state, ticket state, verification state, communication state, or ticket knowledge. Standalone markdown plans do not count as durable knowledge.

## Commands

### Create A Temporary Plan Artifact

Use:

```sh
ctox plan draft --title "<short label>" --prompt "<user request>" [--skill "<skill name>"]
```

Behavior:

- Produces a structured plan artifact only.
- Does not emit queued work.
- Does not persist hidden planning traces.
- Returns only the compact plan result, not intermediate reasoning.

### Persist A Long-Running Plan

Use:

```sh
ctox plan ingest --title "<short label>" --prompt "<user request>" [--thread-key "<thread>"] [--skill "<skill name>"] [--auto-advance] [--emit-now]
```

Behavior:

- Decomposes the raw request into multiple plan steps.
- Stores the plan in CTOX runtime state.
- Does not emit queued work unless `--emit-now` is passed explicitly.
- Does not hook itself into the normal execution path unless you intentionally use the emitted plan commands later.

### Inspect Plan State

Use:

```sh
ctox plan list
ctox plan show --goal-id "<goal_id>"
```

### Force The Next Step

Use:

```sh
ctox plan emit-next --goal-id "<goal_id>"
ctox plan tick
```

### Update Step Outcome

Use:

```sh
ctox plan complete-step --step-id "<step_id>" --result "<short outcome>"
ctox plan fail-step --step-id "<step_id>" --reason "<failure>"
ctox plan block-step --step-id "<step_id>" --reason "<blocker>"
ctox plan retry-step --step-id "<step_id>"
ctox plan unblock-step --step-id "<step_id>" [--defer-minutes "<n>"]
```

## Operating Pattern

1. If the request is long-running, create a plan instead of solving it only in-memory.
2. Prefer `ctox plan draft` for planning mode.
3. After planning, use `ctox queue add` for the concrete bounded work steps that should really enter the queue.
4. Persist with `ctox plan ingest` only when you truly want explicit durable plan state in addition to the queue.
5. Keep the plan title short and stable.
6. Put the full owner intent into `--prompt`; let CTOX decompose it.
7. If a specific CTOX skill should steer later work, pass it via `--skill`.
8. If the work belongs to an existing durable thread, pass `--thread-key`.
9. If the plan assumes ticket history, desk skills, runbooks, or monitoring knowledge, verify those CTOX runtime prerequisites first. If they are missing, make the plan explicitly include onboarding / knowledge build steps rather than assuming a mature system.

## Do Not

- Do not create a plan for tiny one-shot tasks.
- Do not confuse plan artifacts with executable queue items.
- Do not hide blockers in prose when durable plan state is being used.
- Do not persist long internal step-by-step reasoning. Only persist the compact plan artifact or the explicit durable goal/step state.

## Contracts

Read `references/plan-contracts.md` before using this skill for real work.
