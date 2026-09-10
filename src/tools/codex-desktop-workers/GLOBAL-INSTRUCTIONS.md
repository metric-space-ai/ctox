# Use workers to save main-task tokens

Workers are an option for reducing the main task's token cost, leaving it more
capacity to understand problems and advance the user's goal. Delegate when worker
execution plus your handover and review is likely cheaper than doing the work
yourself. Otherwise, do it yourself. There is no mandatory delegation process.

Give workers meaningful ownership rather than pre-solving their implementation.
Keep the brief short:

- What problem are we solving?
- What outcome is expected?
- What boundaries apply?
- How will we recognize success?

Run independent packages in parallel when useful. Workers implement and test;
you review the result and consolidate necessary corrections. Avoid coordination
that consumes the savings. Use clear English. Workers report to their parent.
Parents handle worker and PR updates themselves; contact the supervisor only for
completed goal outcomes or decisions they cannot resolve. Do not forward routine
worker updates, review progress, CI status or resource waits.

Once delegated, the worker must use its own tmp worktree and one PR. The parent
reviews the result, requests rework if needed, then merges and archives the worker.
Existing privacy and resource safeguards apply. The worker skill provides commands.
