# Self-Improving Review Contract

Use this review shape whenever CTOX changed a skill or a skill-facing helper:

1. `Goal`
   - Why did CTOX change the skill?
2. `Change`
   - Which files or contracts changed?
3. `Evidence`
   - Which tests, live runs, or behavior checks support the outcome?
4. `Outcome`
   - `successful`, `partially_successful`, or `failed`
5. `Learning`
   - What did CTOX learn about the system, tool contract, or skill family?
6. `Owner report`
   - Only if the outcome is `successful`

Minimum bar for `successful`:

- at least one direct validation artifact exists
- the new behavior is better than the old behavior in a concrete way
- the learning is durable enough to belong in the ledger

Use `partially_successful` when:

- the patch improved part of the behavior
- but the target behavior is not yet fully trustworthy

Use `failed` when:

- the review evidence does not support the intended result
- or the change introduced a new blocker
