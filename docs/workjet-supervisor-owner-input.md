# Durable Supervisor Owner follow-ups

The existing Guest/WebRTC project control channel accepts
`project.supervisor.turn.input`. Native command:
`ctox.workjet.project.supervisor.turn.input`.

Discover support with the scoped `project.supervisor.turn.capabilities`
request and `includeInput: true`. The default v1 capability response stays
unchanged for installed decoders. Opt-in support returns
`inputContract: ctox.workjet.supervisor_input.v1`,
`inputDelivery: next_slice`, and `maxInputChars: 4096`.

Input uses the existing projectId, threadId and targetCommandId plus body
and a stable operationId. It is admitted by the active verified Owner
binding into the existing command/task. A retry of the same operation
has the same input ID and sequence; changed intent is rejected. Terminal,
foreign and external-worker targets are rejected explicitly.

The response records the actual native turn and admitted input, with
`delivery: next_slice` and `worker_interrupted: false`. Admission does
not revoke a worker, approve a review, change the original prompt or
allocate a new task. A missing-source review can retry promptly; provider
capacity and approval waits keep their original holds.

Each actual native worker captures an immutable input high-water before
its provider invocation. Later input prevents that old context from
closing the task and continues the same task in its next slice. Recovered
snapshots remain unchanged. Context is bounded to 128 inputs / 512 KiB
per task and 4096 Unicode characters per input. Limit failures are explicit.

Input admission is not a work-completion exemption. Existing plan,
artifact, approval, review, lease and exact-attempt guards still apply.
The ledger is durable native task state; it is separate from the historical
v1 turn list and does not claim that an older UI already supports input.
An installed client must discover support before enabling this control.

Conversation completion resolves the original submit actor through the
trusted Workjet identity registry. Only active verified aliases can match
the native canonical Owner; profile/email/display-name claims do not enroll
an alias. Public streaming excludes both reserved fenced crew metadata
and the observed plain `ctox-crew metadata:` tail.
