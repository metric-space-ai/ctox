# Workjet project execution policy intent

Native field: `workjet_projects.execution_policy`. Schema:
`ctox.workjet.project_execution_policy.v1`.

```json
{"schema":"ctox.workjet.project_execution_policy.v1","mode":"autonomous_worktree","revision":1}
```

Modes are `default` (today's behavior) and `autonomous_worktree` (Owner
intent to permit bounded autonomous work). An absent legacy field resolves to
`default` / revision 0, without rewriting the row. Schema migration 4 preserves
existing project IDs, Owners, configuration, schedules and working copies.

This field **does not grant execution authority**. It adds no tools, host access,
provider mode, working directory, allowlist, approval bypass, controller or
scheduler. Until a provider consumes and enforces this policy, the product must
keep its current admission/approval behavior and must not advertise an enforced
autonomous mode.

## Owner configuration and readback

The existing Owner-authorized `ctox.workjet.project.upsert` command accepts:

```json
{
  "project_id":"project-id",
  "name":"Project title",
  "execution_policy":{
    "schema":"ctox.workjet.project_execution_policy.v1",
    "mode":"autonomous_worktree",
    "expected_revision":0
  }
}
```

Identity is rechecked inside the existing domain writer transaction. The caller
cannot supply Owner identity, policy revision, worktree/host paths or tool grants.
Missing `execution_policy` preserves the current field. Null is rejected:
revocation must carry a typed `mode:"default"` patch and the current
`expected_revision`, so a clear cannot silently skip compare-and-swap.

The native revision advances by one only when the mode changes. A fresh command
with the wrong expected revision fails with
`workjet_project_execution_policy_revision_conflict: expected N, current M`
before any project mutation. Re-enabling after revocation gets a new revision;
the fence cannot return to an old revision. The same original command replays its
persisted domain receipt without reapplying a stale patch. JSON-safe revision
exhaustion fails closed.

The Shell façade maps `project.configure.executionPolicy` to that exact native
patch. Its correlated native receipt returns `project.executionPolicy`
with `revision`, never `expected_revision`. Selection alone never returns a
`granted` or `enforced` flag. Configure callers that omit the policy and legacy
list callers keep their previous DTO shape.

`project.list {includeExecutionPolicy:true}` requests the current typed policy
through the existing Owner-correlated RxDB/WebRTC project read; omitted/false
does not expose it. It is independent of `includeConfiguration` and
`includeSupervisorLuma`. The request flag is not sent as native project authority.
A legacy row returns the explicit default/revision0 only on this read opt-in.
Unknown/invalid stored policy never becomes a default autonomy grant.

## Required execution consumer

Harness/worker admission must read fresh native project policy, bind its revision
to the actual project/team thread, and verify the working copy, computer and
worktree before granting bounded ordinary operations. A stale policy revision,
foreign project/thread, unverifiable worktree, unsupported harness, host-secret
access or action outside the verified worktree must retain denial/escalation.
`runtimeMode:"full-access"` and a caller's claimed policy cannot establish this.
Changes during a running execution need revalidation at the actual tool boundary;
a cached client setting cannot survive Owner revocation as authority.

The consumer owns the supported harness/mode and actual enforcement receipt.
Main owns the Owner configuration UI after that consumer exists. Crew owns this
native typed storage seam. Installed acceptance must exercise in-worktree
build/test without a dialog, outside/secret escalation, revocation and
quit/reopen on identified native/Shell/Workjet revisions; source tests alone are
not that acceptance.

## Contract generation

Canonical fixture:
`src/core/rxdb/tests/fixtures/workjet-project-execution-policy-v1.json`.
`node src/core/rxdb/tools/build_workjet_jour_fixe_contract.mjs` generates both the
native serde contract and browser validator. The module collection schema, native
schema contract/hash registry and RxDB bundle are regenerated through their
existing generators; there is no additional projection chain or HTTP data path.
