# Native Workjet supervisor identity

`ctox.workjet.project.supervisor.bind` accepts `{project_id, thread_id}` from
an authenticated project owner. `thread_id` is the canonical UUID of the
**existing** Workjet supervisor CodeThread. No working copy or computer is
required, and no new CodeThread, transfer session, coding-session identity,
provider grant or model invocation is manufactured.

The result is `{ok, contract: "ctox.workjet.supervisor_binding.v1", binding:
{project_id, thread_id, thread_key}}`. The route is the native Threads
producer's actual `business-os/threads/<UUID>` key. Send later messages through
the existing `threads.ai.request` command using that `thread_id`; registration
alone does not execute a model turn or confirm live provider availability.

A native-only SQLite identity registry, the corresponding `user_threads`
source record and the domain-effect receipt commit in one IMMEDIATE transaction.
Existing command admission, project ownership, terminal replay and publication
recovery apply. The returned binding is stable across retries and later owner
renames; registering again does not reset messages or UI state. Browser metadata
alone is never evidence of registration.

Archived/foreign projects, unknown payload fields, non-UUID IDs, replacing a
bound supervisor, adopting unrelated native chat history, or using one UUID
for another project fail before a binding effect commits. An existing binding
with missing/deleted/incompatible source history requires repair rather than
silently rebuilding the history. UUIDs and project titles do not establish
ownership. Existing Workjet history remains in its original CodeThread; this
endpoint does not import, clone or modify that history.

Workjet Main must call this command for each existing supervisor and retain the
correlated native result rather than filling `ctoxSession` with a guessed transfer
ID. Installed acceptance must verify a real supervisor message and its reply
on the same UUID after reload/reopen. Per-project Jour-fixe schedule reconciliation
and prompted-KPI resolver tools consume the native binding in follow-up changes;
those behaviors are not implemented by registration alone.
