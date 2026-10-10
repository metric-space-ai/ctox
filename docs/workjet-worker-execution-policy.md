# Workjet worker project policy reference

The optional field on the existing `business_os.remote_worker_admission`
binding is `executionPolicy`:

```json
{"mode":"autonomous-worktree","projectId":"project-id","revision":1}
```

Its canonical native/browser fixture is
`src/core/rxdb/tests/fixtures/workjet-worker-execution-policy-v1.json`, schema
`ctox.workjet.worker_execution_policy.v1`. The fixture generates the serde
reference and browser validator through the existing contract generator.
Revision is a positive JSON-safe integer. Null, extra fields, another project
and the native storage spelling `autonomous_worktree` are rejected in this
reference. Omitting the field retains existing admission and receipt bytes.

## Fresh native authority

Issue, claim, revalidate and renew check the current Owner, active owned project,
native repository binding, assigned computer/build capability and registered
target tuple inside the existing admission transaction. A present reference
also requires the current native `workjet_projects.execution_policy` to be
`autonomous_worktree` with exactly its referenced revision. Owner reset or
re-enabling with a newer revision invalidates the previous reference.

The supplied `sourceSupervisorThreadId` must join the real
`workjet_supervisor_bindings` registry for this Owner and project and the
current native `user_threads` source provenance. An arbitrary external Workjet
Parent ID, copied role or browser metadata does not satisfy that join.
This uses the existing native supervisor-bind command's output, not a new
projection chain. A Parent that has not actually been enrolled natively is
denied. Broader Parent enrollment is a separate producer dependency; callers
must not substitute the root Supervisor ID for a Parent.

The receipt echoes the exact immutable reference after these checks. It
participates in the authority fingerprint and lost-ACK correlation. Changed
references cannot reuse an existing permit. Revoke still authenticates the
actual source Owner and exact receipt binding but does not require a still
enabled project policy, native team membership or unexpired permit.

## Execution remains the consumer's responsibility

This receipt is source admission, **not a confinement receipt**. It grants no
path, approval bypass, host effect, provider account or controller. Actual
worktree verification, tool-boundary revalidation, process/file/MCP containment
and outside-worktree escalation belong to Harness and the target adapter.
Model/account references remain locators subject to the source gateway's
current account grant. No execution adapter is advertised as supporting
autonomous-worktree on the strength of this change; supported modes remain
empty until genuine installed enforcement is measured.

Existing requests without the field remain unchanged. Configure/list roundtrip
and policy CAS are described in `workjet-project-execution-policy.md`.
