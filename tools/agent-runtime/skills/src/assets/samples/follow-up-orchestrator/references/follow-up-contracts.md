# CTOX Follow-up Contracts

This document defines the boundary between end-of-turn evaluation and actual next-step execution.

## Evaluation Contract

- Follow-up evaluation happens after a meaningful execution slice.
- The tool must return only a compact structured decision.
- Internal reasoning about why the decision was made must not be persisted as durable context by default.

## Allowed Outcomes

- `done`
- `needs_followup`
- `blocked_on_user`
- `blocked_on_external`
- `needs_replan`

No other status should be relied on as stable workflow state.

## Follow-up Proposal Contract

When the result is `needs_followup`, the tool may propose:

- `follow_up_title`
- `follow_up_prompt`
- `suggested_skill`
- `suggested_thread_key`

This is a proposal, not an automatic enqueue action.

If the caller decides the follow-up should really run later, the caller should create or edit an explicit queue task.

## Communication Contract

- The evaluation tool may recommend owner communication.
- It must not send the message itself.
- If owner communication is warranted, the calling agent should use the existing communication tools or skills after seeing the structured evaluation result.

## Replan Contract

- If requirements or assumptions changed materially, return `needs_replan` instead of guessing the next task from stale context.
- Replanning should normally use `ctox plan draft` first and only persist a new plan if needed.

## Completion Contract

- Return `done` only if the active scope is actually closed.
- If there is a real next slice, prefer `needs_followup`.
- If the next slice is unclear, prefer `needs_replan` or an explicit blocker over guessing.
