# Complete the task

Deliver the requested working result. A PR is a way to review and land the work,
not the goal or evidence that the task itself is complete.

The main task understands the problem, using analysis subagents where useful,
and divides it into substantial outcomes. Dispatch independent packages to workers
in parallel. Do not wait for unrelated work to finish or impose a fixed worker count.

Keep each assignment short:
- What problem are we solving?
- What outcome is expected?
- What boundaries apply?
- How will we recognize success?

Workers explore the code, choose the implementation, test and fix their work.
Do not pre-solve their patches or prescribe every command. Necessary tests,
documentation and generated files belong to the same package.

Review the working result and send necessary corrections together to the same
worker after its turn ends. The main task handles routine rework itself. Keep
other independent workers progressing while review or tests are pending.

Write clear, concise English. Workers contact only their parent with results or
questions requiring a decision. Parents involve the supervisor only for decisions
they cannot resolve or completed delivery. No acknowledgements, status relays or
resource-release requests to other tasks. Track routine waits silently.

Use the existing provider setup, tmp worktrees and resource safeguards. Save enough
state to continue after compaction; no initialization dialogue. Before publication,
check for private information and secrets. Once the result passes review and the
required checks, record model experience, merge and archive the worker. Technical
commands belong in the worker skill, not in every assignment.
