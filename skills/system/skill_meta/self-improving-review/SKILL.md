---
name: self-improving-review
description: Use after CTOX creates, edits, or refines skills, helpers, or skill contracts and needs to verify that the self-optimization actually worked, document the learning, and report the successful improvement to the owner.
metadata:
  short-description: Review successful skill self-improvement before reporting it
cluster: skill_meta
---

# Self-Improving Review

## CTOX Runtime Contract

- Task spawning is allowed only for real execution slices that add mission progress, external waiting, recovery, or explicit decomposition. Do not spawn work merely because review feedback exists.
- The Review Gate is a quality checkpoint, not a control loop. After review feedback, continue the same main work item whenever possible and incorporate the feedback there.
- Do not create review-driven self-work cascades. If more work is needed, reuse or requeue the existing parent work item; create a new task only when it is a distinct slice with a stable parent pointer.
- Every durable follow-up, queue item, plan emission, or self-work item must have a clear parent/anchor: message key, work id, thread key, ticket/case id, or plan step. Missing ancestry is a harness bug, not acceptable ambiguity.
- Rewording-only feedback means revise wording on the same artifact. Substantive feedback means add new evidence or implementation progress. Stale feedback means refresh or consolidate current runtime state before drafting again.
- Before adding follow-up work, check for existing matching self-work, queue, plan, or ticket state and consolidate rather than duplicating.


Review learnings only count as durable knowledge when they are reflected in SQLite-backed runtime state such as verifications, continuity, ticket knowledge, or other runtime store records. Standalone notes do not count as durable knowledge by themselves.

Use this skill whenever CTOX changed one or more skills, helper scripts, skill contracts, or skill-facing runtime rules and now needs to decide whether that self-optimization was genuinely successful.

This skill is the review and reflection layer around refinement. It exists to stop CTOX from claiming "self-improvement" just because files changed.

## Core Rule

Do not report a successful self-optimization to the owner until all three conditions are true:

1. the intended skill change was actually applied
2. there is concrete evidence that the change improved or repaired the behavior
3. the learning was documented in the skill-improvement ledger

If any of those fail, do not send a success report.

## When To Use

- after creating a new skill
- after refining or patching an existing skill
- after changing helper scripts used by a skill
- after changing skill-level tool contracts
- after changing prompt or routing rules that directly alter skill behavior

## Required Inputs

Gather these before concluding the review:

- the goal of the skill change
- the exact skill files or helper files changed
- the validation evidence
- the resulting behavior change
- whether the owner should be informed now

## Workflow

1. Identify the exact skill mutation.
2. State the intended outcome in one sentence.
3. Review the changed files and validation evidence.
4. Decide one of:
   - `successful`
   - `partially_successful`
   - `failed`
5. If the outcome is not `successful`, stop there:
   - document the failed or partial outcome only if that is useful internally
   - do not send a success report to the owner
6. If the outcome is `successful`:
   - append a structured learning entry to `contracts/history/skill-improvement-ledger.md`
   - record the resulting lifecycle or promotion step with `skill-lifecycle`
   - use `owner-communication` on the primary owner channel to report the successful self-optimization
7. The owner-facing report must say:
   - what changed
   - why CTOX changed it
   - what concrete evidence says it now works better
   - what is now safer, clearer, or more capable than before

## Documentation Helper

Prefer the open helper:

```sh
python3 skills/system/skill_meta/self-improving-review/scripts/record_skill_review.py \
  --ledger contracts/history/skill-improvement-ledger.md \
  --status successful \
  --summary "<what changed>" \
  --goal "<why the refinement existed>" \
  --evidence "<validation evidence>" \
  --skills "<comma-separated skills>"
```

If the helper does not fit, update the ledger manually in the same structured format.

## Owner Report Contract

Only report successful self-improvement if the review outcome is `successful`.

The owner report must not be vague. It should clearly separate:

- reviewed goal
- successful change
- evidence of improvement
- current impact

Bad:

- "CTOX improved itself."
- "I optimized some skills."

Good:

- "CTOX refined the mail-context workflow, validated it with communication history lookups, and can now actively reconstruct prior thread state before answering."

## Do Not

- Do not treat a file edit alone as success.
- Do not report success when the new behavior is only hypothesized.
- Do not skip validation.
- Do not send a self-improvement victory message if the change is still blocked, degraded, or only partially working.
- Do not overwrite the creation ledger with skill-specific learnings; use the dedicated skill-improvement ledger for that.

## References

- Read `references/review-contract.md`.
- Write structured learnings to `contracts/history/skill-improvement-ledger.md`.
- Coordinate lifecycle transitions with `../skill-lifecycle/SKILL.md`.
