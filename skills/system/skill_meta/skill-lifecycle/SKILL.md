---
name: skill-lifecycle
description: Track the lifecycle state of CTOX skills from draft through candidate and promoted use, document why a skill changed state, and keep skill evolution reviewable instead of implicit.
metadata:
  short-description: Manage skill promotion and deprecation states
cluster: skill_meta
---

# Skill Lifecycle

## CTOX Runtime Contract

- Task spawning is allowed only for real execution slices that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct slice with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Skill lifecycle changes are not durable mission knowledge by themselves. The lasting operational state must still be visible in SQLite-backed runtime state, bindings, knowledge records, or verification records.

Use this skill whenever CTOX creates, adopts, promotes, deprecates, or materially rewrites a skill.

This skill exists so that skill growth does not become "some folder changed on disk". Every meaningful skill mutation should have a visible lifecycle state and a reason.

## Lifecycle States

- `draft`
  - newly created or heavily rewritten
  - not yet trusted beyond local experimentation
- `candidate`
  - passed initial validation
  - acceptable for targeted use, but still under observation
- `promoted`
  - reviewed, validated, and trusted as a normal reusable skill
- `deprecated`
  - still present for continuity, but should not be preferred for new work
- `retired`
  - intentionally superseded or removed from active use

## Core Rules

1. Do not treat a new skill as implicitly promoted.
2. When a skill changes state, document:
   - which skill changed
   - from which state to which state
   - why
   - what validation or evidence supported the transition
3. Pair `promoted` transitions with `self-improving-review` if the change came from self-optimization.
4. Use `deprecated` rather than silently abandoning an older skill when it still matters for continuity.

## Workflow

1. Identify the skill and the exact lifecycle transition.
2. Review the validation evidence and current trust level.
3. Decide the correct target state.
4. Append the transition to `contracts/history/skill-lifecycle-ledger.md`.
5. If the state is `promoted`, ensure the owner-facing report is aligned with `self-improving-review`.

## Helper

Prefer the open helper:

```sh
python3 skills/system/skill_meta/skill-lifecycle/scripts/record_skill_lifecycle.py \
  --ledger contracts/history/skill-lifecycle-ledger.md \
  --skill "<skill-name>" \
  --from-state "<old-state>" \
  --to-state "<new-state>" \
  --reason "<why this changed>" \
  --evidence "<what validation supports it>"
```

## References

- Read `references/lifecycle-states.md`
- Write transitions to `contracts/history/skill-lifecycle-ledger.md`
