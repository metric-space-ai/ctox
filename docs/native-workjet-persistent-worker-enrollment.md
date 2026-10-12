# Native Workjet Persistent Worker enrollment

This is an additive native Owner command contract. It does not replace the
single project Supervisor, enable autonomous worktrees by itself, or introduce
a new RxDB collection/projection chain.

## Producer

The normal authenticated Business OS command path accepts:

- ctox.workjet.project.persistent_worker.bind
- ctox.workjet.project.persistent_worker.revoke

Payload uses snake_case and rejects unknown fields:

    {
      "project_id": "<owned native project id>",
      "supervisor_thread_id": "<existing native Supervisor CodeThread UUID>",
      "thread_id": "<the actual Workjet Persistent Worker CodeThread UUID>",
      "expected_revision": 0,
      "title": "Persistent Worker"
    }

Initial binding requires revision 0 and no existing native history at this UUID.
Reconfirm/revoke/rebind require the exact current revision. Reconfirming an
active binding preserves history and revision. Revoke increments revision and
sets state to revoked. Explicit rebind increments it again; an old reference
never becomes valid again. A command replay uses the existing DomainEffect
receipt and does not reset title, messages or timestamps.

The command derives the canonical Owner from the authorized session, checks
the owned active project and its existing native Supervisor binding, and commits
private provenance plus the existing user_threads source record in the same
DomainEffect transaction. No caller can supply owner, role or revision output.
A foreign UUID, unrelated existing history, noncanonical UUID, different project
or different Supervisor is rejected. Revocation remains possible on an archived
owned project. There is still exactly one native Supervisor per project.

Result.binding is the complete immutable-affiliation, revision-bound reference:

    {
      "contract": "ctox.workjet.persistent-worker-binding.v1",
      "owner_user_id": "<canonical native Owner>",
      "project_id": "<native project id>",
      "supervisor_thread_id": "<native Supervisor UUID>",
      "thread_id": "<actual Persistent Worker UUID>",
      "role": "persistent_worker",
      "revision": 1,
      "state": "active"
    }

user_threads.metadata carries workjet_team_role and workjet_team_binding for
presentation and consistency checks. Authority comes from the private native
workjet_persistent_worker_bindings registry, never from replicated metadata.
The existing Threads projection uses the current DomainEffect admission path.
There is no extra publisher, queue-to-mirror chain or schema generation.

## Source admission consumer

The native admission primitive is:

    mcp_workjet_worker_dispatch::current_project_with_team_binding(
        policy, request_context, project_id, actual_source_thread_id,
        Some(&owner_issued_binding)
    )

A Persistent Worker consumer must pin the complete binding in its permit
binding/digest and pass that same reference on issue, claim, revalidate and
renew. It must NOT substitute supervisor_thread_id as the calling Parent UUID.
The helper validates the native registry, exact reference/revision, current
project ownership and active state, current thread role/history, native
Supervisor provenance and current actor permissions. Changed role, revoked
binding, foreign/project mismatch, deleted history, closed/archived Parent or
archived project fails closed. Root Supervisor callers keep using current_project;
without an explicit Parent reference a Parent UUID is still rejected.

This PR provides the native enrollment producer and shared admission primitive.
The autonomous-worktree consumer in #590 must bind this reference before
enabling its Parent path. Workjet must issue the Owner command for its actual
Persistent Worker UUID through the normal product path; no production pairing
or grant is created by this change. Full installed remote execution acceptance
remains outstanding until those consumers are wired and delivered.
