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

For a Supervisor, `sourceSupervisorThreadId` joins the existing native
Supervisor registry and current history. For a Persistent Worker, that legacy
field name retains the **actual calling Parent UUID**. The optional
`parentTeamBinding` carries the complete `result.binding` produced by
`ctox.workjet.project.persistent_worker.bind`:

```json
{
  "contract": "ctox.workjet.persistent-worker-binding.v1",
  "owner_user_id": "<canonical Owner>",
  "project_id": "<native project>",
  "supervisor_thread_id": "<native Supervisor UUID>",
  "thread_id": "<actual Parent UUID>",
  "role": "persistent_worker",
  "revision": 1,
  "state": "active"
}
```

The same fixture generates its strict native/browser wire types. Each issue,
claim, revalidate and renewal compares the full reference against the private
native Parent registry, exact Owner/project/Root affiliation and current open
Parent history/role. Root UUID substitution, missing references for a known
native Parent, revision changes, revoke/rebind, closed/archived/deleted history
and foreign affiliation fail closed. A copied metadata role cannot enroll a
Parent. The producer is documented in
[native Parent enrollment](native-workjet-persistent-worker-enrollment.md).
This consumer adds no publisher, collection, enrollment or permission bypass.

A Parent reference is pinned even without an autonomous-worktree policy.
The full reference and actual calling UUID participate in the immutable permit
and current authority fingerprint. Requests omitting both references keep
their legacy receipt/fingerprint bytes; a known native Parent cannot use that
compatibility path to omit its reference.

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

Existing requests without either reference remain unchanged. Configure/list roundtrip
and policy CAS are described in `workjet-project-execution-policy.md`.
