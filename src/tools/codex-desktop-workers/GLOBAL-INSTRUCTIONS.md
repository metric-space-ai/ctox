# Working with implementation workers

The main task understands the problem, uses analysis subagents when helpful and
breaks the goal into coherent packages. It dispatches independent packages in
parallel; there is no fixed worker-count limit.

An assignment briefly answers four questions:

- What problem are we solving?
- What outcome is expected?
- What boundaries apply?
- How will we recognize success?

Add only useful references. Do not pre-solve the implementation, prescribe command
sequences or repeat process rules. The worker explores the code, chooses the
implementation and delivers the result with necessary tests. Keep related work
in one package.

The main task reviews the result and sends corrections together to the same
worker. Routine rework needs no supervisor approval. Write clear, concise English
messages; put technical identifiers in separate supporting details only when needed.

Each worker delivers a PR. Before publication, the main task reviews changes and
text for private information and secrets. After successful review and required
tests, it briefly records the model experience, merges and archives the worker.
PR administration supports completed work.

Workers report results or questions requiring a decision only to their main task,
then end the turn. Resource waits belong in the saved status; do not message other
tasks or resource owners asking for release, updates or acknowledgements. The
existing supervisor monitor reads resource status without starting message chains.
Corrections start a new turn in the same worker. The main task reports relevant
results or unresolved blockers to the supervisor without waiting for an
acknowledgement. No waiting loops or unchanged status messages.

Save the assignment and progress durably, and resume after compaction. No READY
or initialization dialogue. Worktrees and build data belong on the tmp volume;
existing resource rules also apply to parallel workers.

Choose models by experience and availability. Quotas are temporary blockers, not
model weaknesses. OpenAI stays directly connected; other models use their own
provider. Look up technical operations in the `proxy-model-workers` skill when
needed; do not copy them into every assignment.
