# CTOX Plan Contracts

This document defines the contract between Codex and the explicit CTOX plan layer.

## Goal Contract

- A goal is the durable envelope for a multi-step owner request.
- A goal must have one stable short title and one full original prompt.
- A goal may optionally declare one preferred skill name.
- A goal advances step-by-step; it is not a free-form notebook.
- A plan is not the queue itself.

## Step Contract

- Exactly one step per goal should be `queued` at a time when explicit plan emission is used.
- `pending` means eligible for future emission.
- `queued` means emitted into the normal inbound routing path but not yet running.
- `completed` means the step finished with a concrete result excerpt.
- `blocked` means execution cannot proceed without an explicit unblock action.
- `failed` means execution attempted the step and did not succeed; retry must be explicit.

## Emission Contract

- The plan layer emits follow-up work through the same inbound routing substrate used for cron, email, Jami, and TUI.
- Plan work must therefore stay compact and self-contained.
- Each emitted step must carry plan metadata: `goal_id`, `step_id`, `step_order`, `total_steps`, and optional `skill`.

## Skill Contract

- A preferred skill is advisory steering, not proof that the skill is available.
- If the requested skill is missing, continue with the best fallback and note the mismatch.
- Do not silently swap a plan to a different skill name unless the owner or repo policy justifies it.

## Owner-Input Contract

- Use plan ingestion when the owner request is long-running, staged, or interruption-prone.
- Preserve the owner's original request in the goal prompt; do not replace it with an abstract rewrite.
- If the request is ambiguous, the first plan step may be a clarification or inspection step, but that must be explicit.
- If the next executable work step should survive beyond the current turn, create it explicitly in the queue rather than assuming the plan itself will run.

## Blocker Contract

- If progress depends on missing owner input, credentials, external approvals, or unavailable infrastructure, mark the step `blocked`.
- A blocked step must carry a short actionable blocker reason.
- Do not mark blocked work as completed.

## Failure Contract

- A failed step records the most recent failure excerpt.
- Failure does not automatically retry.
- Retry is explicit through `ctox plan retry-step`.

## Completion Contract

- A step completion should persist a short concrete outcome, not a full transcript dump.
- When all steps are completed, the goal becomes `completed`.
- Goal completion must not destroy the historical step trail.
