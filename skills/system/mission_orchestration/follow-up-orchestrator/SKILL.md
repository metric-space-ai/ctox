---
name: follow-up-orchestrator
description: Use at the end of a meaningful multi-step turn to decide whether the work is actually complete, blocked, needs replanning, or should produce a concrete follow-up task proposal.
metadata:
  short-description: Evaluate completion and propose follow-up work
cluster: mission_orchestration
---

# Follow-up Orchestrator

## CTOX Runtime Contract

- Task spawning is allowed only for real execution slices that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct slice with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Use this skill near the end of a non-trivial turn when you need an explicit completion judgment instead of quietly stopping.

## Core Rule

Do not silently abandon unfinished work. If the current turn materially advanced the task but did not close it, use the follow-up tool to return a compact structured judgment.

Durable mission understanding lives in the CTOX runtime store. SQLite-backed continuity, ticket state, plan state, communication records, verification runs, and ticket knowledge count as durable knowledge. Standalone markdown notes or workspace files do not count as knowledge by themselves.

## Command

```sh
ctox follow-up evaluate --goal "<goal>" --result "<latest result>" [--step-title "<step>"] [--skill "<skill>"] [--thread-key "<thread>"] [--blocker "<reason>"] [--open-item "<item>"]... [--requirements-changed] [--owner-visible]
```

## Output Meaning

- `done`: current scope is complete
- `needs_followup`: the work advanced but a concrete next slice still exists
- `blocked_on_user`: the owner must answer or approve something
- `blocked_on_external`: an external dependency blocks progress
- `needs_replan`: assumptions or requirements changed enough that the existing path is no longer reliable

## Harness Signals to Consult

Before classifying a turn as `done` or `needs_followup`, run:

```sh
ctox harness-mining stuck-cases --min-attempts 5 --limit 20
```

Read `cases[].entity_id` and `rejected_attempts`. If any entity touched by this
turn appears in the list, the turn is not `done` even if the visible work
finished — it is `needs_followup` with an `--open-item "stop retry-loop on
<entity_id> (<N> rejected attempts)"`. A turn that closes the foreground task
while leaving a hot retry-loop downstream is misclassified by definition.

## Workflow

1. Finish the meaningful execution slice first.
2. Summarize the latest concrete result compactly.
3. If there are explicit remaining items, pass them via repeated `--open-item`.
4. If the problem is now blocked, pass `--blocker`.
5. If the requirements shifted during execution, pass `--requirements-changed`.
6. If the owner should probably be informed, pass `--owner-visible`.
7. If the result is `needs_followup`, choose the durable runtime primitive before you end the turn:
   - queue-only for tiny atomic follow-up
   - ticket self-work plus queue or plan for multi-turn, review, approval, blocker, or recovery work
8. If the work is high-impact, externally visible, owner-facing, or likely to span multiple turns, do not merely promise a "next step". Create the explicit self-work or review task before you end the turn.
9. If execution started but did not reach a safe verified end state, record a review or recovery slice immediately. Prefer ticket self-work over queue-only when the recovery may need tracking, approval, or repeated follow-up.
10. If the blocker depends on owner input, enumerate the exact missing values, credentials, approvals, or decisions in the owner-facing status. Do not send vague blocker summaries.
11. For owner-visible blocked work, prefer a durable review schedule over waiting in the active turn.
12. Do not create repeat owner-facing blocker communication unless there is a material delta since the last owner update. If nothing changed, keep the next review internal and durable.
13. Before creating follow-up for ticket-bearing or knowledge-bearing work, inspect whether the ticket system and knowledge plane are actually operational:
   - `ctox ticket sources`
   - `ctox ticket source-skills`
   - `ctox ticket knowledge-list --system "<system>" --limit 20`
   - `ctox ticket self-work-list --system "<system>" --limit 20`
   If the full ticket+knowledge pipeline is not active, state that plainly in the follow-up instead of pretending durable knowledge already exists.
14. If the only persisted artifact is a workspace note or markdown file, treat the knowledge task as still open. Persist the mission understanding into SQLite-backed runtime stores before calling the work durable.

## Important Separation

- This tool evaluates the turn outcome.
- It does not send communication itself.
- It does not automatically enqueue work.
- It does not reorder the queue for you.
- After the evaluation, you may:
  - create the next explicit queue task with `ctox queue add`
  - create ticket-backed durable follow-up with `ctox ticket self-work-put ... --publish` and `ctox ticket self-work-assign ...`
  - reprioritize or update existing queued work with `ctox queue edit` or `ctox queue reprioritize`
  - send an owner update with the communication tools
  - draft a new plan with `ctox plan draft`
  - persist another long-running plan with `ctox plan ingest` only if explicit plan state is still needed

## Do Not

- Do not use this for tiny one-shot tasks that are obviously done.
- Do not invent speculative future work just because more work could exist in theory.
- Do not use it to bypass owner approval.
- Do not persist evaluation reasoning beyond the compact structured result.
- Do not tell the owner that CTOX is actively doing a next step unless the corresponding durable self-work, queue, or plan state exists.

## Contracts

Read `references/follow-up-contracts.md` before using this skill for real work.
