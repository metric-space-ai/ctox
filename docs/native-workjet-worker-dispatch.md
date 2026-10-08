# Native supervisor → Workjet worker dispatch

The native project supervisor runs through Threads and the existing service
queue. It has no Workjet source MCP session for workjet_dispatch_worker.
This adapter connects it to the existing Workjet dispatcher through one
durable control queue. CTOX creates no second worker, worktree or harness.

## Source service contract

Use the existing authenticated managed Business OS MCP connection.
Tool: business_os.workjet_worker_dispatch.
The source grant needs allowWrites:true and, if allowedTools is nonempty,
business_os.workjet_worker_dispatch plus business_os.remote_worker_admission.
Native tools/list advertises both; the gateway must include both in WRITE_TOOLS.
No collection read/write, approval or external-effect grant is added by this
adapter. Native current persisted Owner/Admin, integrations.manage workspace
and ctox.task.create on the owned project remain required. The managed grant
cannot invoke dispatch: that operation requires the signed native supervisor
session. If the existing pairing grant lacks these tools, pair a separate
narrow source client; never rotate/widen an existing connector token. Actual
live grant availability must be checked by the source owner, not inferred
from tool registration. No credential is printed or returned.
Receipt contract: ctox.workjet.worker-dispatch.v1.
Request operation/field names are snake_case; receipt fields are camelCase.

- register_source: source_environment_id, source_supervisor_thread_id,
  project_id, optional expected_revision.
- poll: source_environment_id. At most one oldest pending intent plus its
  matching registration; an empty queue returns intents:[].
- complete: registration_id, revision, intent_id, result.
  ACK includes contract, registrationId, revision, intentId.
- revoke_source: registration_id, revision.

Registration returns registrationId, revision, sourceEnvironmentId,
sourceSupervisorThreadId, sourceInstanceId, projectId, ownerUserId,
authorityEpoch, state. Source instance comes from authenticated MCP context.
Supervisor must be the actual CodeThread UUID in workjet_supervisor_bindings,
with current owned active project and canonical Threads provenance.
Browser metadata cannot create this provenance. Workjet additionally derives
the current local orchestrator and pairing from its existing projection.

An identical active registration retains its ID/revision. Replacement,
changed authority epoch, or explicit reactivation requires the exact prior
expected_revision. Ordinary retries cannot revive a tombstone. The source
must never automatically supply that revision to bypass a revocation.

One bounded poll loop per source connection suffices. No per-project watchers,
callback credentials or browser HTTP bridge are needed. Poll checks current
native owner, project, supervisor, policy and registration. Replaced/revoked
and old-epoch registrations cannot deliver old intents. Project/provenance
denial is surfaced: the source must resolve or revoke that registration.

Intent: intentId, registrationId, registrationRevision, sourceEnvironmentId,
sourceSupervisorThreadId, projectId, task, optional title/computerId/
workerProfileId. Repeated polls retain the same UUID. Invoke the EXISTING
WorkerDispatch using intentId as its trusted first-use request/worker ID.
Persist immutable request/result identity before ACK; retries must not create
a second worker or silently resolve different target/model scope.

Success result is the existing typed RemoteWorkerResult: schemaVersion:1,
status:dispatched, environmentId, workerThreadId, computerId, branch,
worktreePath, parent:{environmentId,threadId}, modelSelection:{instanceId,
model,options?:[{id,value:string|boolean}]}, enabledCapabilityIds.
Worker ID equals intent UUID; parent equals registered source supervisor;
an explicitly selected computer must match. Failure: schemaVersion:1,
status:failed, reason from existing terminal WorkerDispatch reasons.
remote-dispatch-pending is not terminal. Unknown fields, duplicate options/
capabilities, oversized or changed completion are rejected. Exact replay
is idempotent.

## Native supervisor contract

Only a registered native project-supervisor business_os.chat.task receives
the restricted signed MCP session. Its only tool operation is dispatch:
dispatch_key (stable within the command), task (bounded), optional
title/computer_id/worker_profile_id. The model cannot supply owner, project,
parent, registration or source identity. Those come from the native command
and current registration.

The signed session binds capability epoch and exact routing task, lease
owner, leased-at, worker-instance ID. Dispatch rechecks canonical command/
payload hash, unexpired exact lease, native provenance and policy. The core
transaction fences cancellation/lease replacement through intent commit;
native policy remains held through that commit. Exact dispatch_key replay
returns the same intent/result; changed scope/task is rejected. At most128
uncompleted intents per owner under current registration revisions and
current authority epoch are admitted; retired intents do not consume capacity.

The pending receipt proves admission only. Internal sessions cannot control
source registrations or mint remote worker permits. Ordinary command bounds
remain intact. An admitted dispatch persists after UI or originating turn
ends. Remote execution still requires the current target registration and
permit (#400/#403), native computer policy and the source ProviderGateway's
exact current account/model grant. The queue is no execution permit.

## Verification and limits

Focused regressions enter the actual MCP dispatcher/native command plane:
supervisor binding/turn/queue identity, signed restricted session, lease
replacement/expiry/cancellation, owner/epoch/project/supervisor retirement,
source isolation/revisions/tombstones, lost ACKs and exact typed completion.

Compilation and installed A0 acceptance remain separate evidence levels.
This adapter does not implement guest restoration or establish goals15/16/18.
Unknown/pending external effects remain rejected in checkpoint handoff.
