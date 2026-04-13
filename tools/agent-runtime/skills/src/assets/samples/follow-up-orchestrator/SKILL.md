---
name: follow-up-orchestrator
description: Use at the end of a meaningful multi-step turn to decide whether the work is actually complete, blocked, needs replanning, or should produce a concrete follow-up task proposal.
metadata:
  short-description: Evaluate completion and propose follow-up work
---

# Follow-up Orchestrator

Use this skill near the end of a non-trivial turn when you need an explicit completion judgment instead of quietly stopping.

## Core Rule

Do not silently abandon unfinished work. If the current turn materially advanced the task but did not close it, use the follow-up tool to return a compact structured judgment.

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

## Workflow

1. Finish the meaningful execution slice first.
2. Summarize the latest concrete result compactly.
3. If there are explicit remaining items, pass them via repeated `--open-item`.
4. If the problem is now blocked, pass `--blocker`.
5. If the requirements shifted during execution, pass `--requirements-changed`.
6. If the owner should probably be informed, pass `--owner-visible`.
7. If the result is `needs_followup`, explicitly create or edit the next queue task with `ctox queue add` or `ctox queue edit`.
8. If the work is high-impact, externally visible, or owner-facing, do not merely promise a "next step". Create the explicit follow-up or review task before you end the turn.
9. If execution started but did not reach a safe verified end state, record a review or recovery slice in the queue immediately.
10. If the blocker depends on owner input, enumerate the exact missing values, credentials, approvals, or decisions in the owner-facing status. Do not send vague blocker summaries.
11. For owner-visible blocked work, prefer a durable review schedule over waiting in the active turn.
12. Do not create repeat owner-facing blocker communication unless there is a material delta since the last owner update. If nothing changed, keep the next review internal and durable.

## Important Separation

- This tool evaluates the turn outcome.
- It does not send communication itself.
- It does not automatically enqueue work.
- It does not reorder the queue for you.
- After the evaluation, you may:
  - create the next explicit queue task with `ctox queue add`
  - reprioritize or update existing queued work with `ctox queue edit` or `ctox queue reprioritize`
  - send an owner update with the communication tools
  - draft a new plan with `ctox plan draft`
  - persist another long-running plan with `ctox plan ingest` only if explicit plan state is still needed

## Do Not

- Do not use this for tiny one-shot tasks that are obviously done.
- Do not invent speculative future work just because more work could exist in theory.
- Do not use it to bypass owner approval.
- Do not persist evaluation reasoning beyond the compact structured result.
- Do not tell the owner that CTOX is actively doing a next step unless the corresponding durable queue or plan state exists.

## Contracts

Read `references/follow-up-contracts.md` before using this skill for real work.
