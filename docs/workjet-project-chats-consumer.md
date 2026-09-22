# Required Workjet project-chat consumer boundary

This is the concrete integration requirement for the existing instance-session
and CTOX DB lifecycle. It does not introduce a new transport or declare the
execution-authority IPC client to be a Business OS data client.

## Verified starting points

Workjet dfcc7fe7bd1f5414277c0a12f5923cd39705df95 has the native execution IPC
client and generated execution contracts, but no delivered project/chat data
consumer. docs/internals/ctox-sync-ipc-consumer.md explicitly describes that
unwired state. Architecture owns the missing production instance-session
integration. The old loopback MCP cross-mode delegate is not the new data path.

Within Business OS, consumers already receive DB/collection handles from the
shell and use shared/sync.js and shared/command-bus.js. A consumer in that session
must use those handles. An independent Workjet Coding UI must receive the same
authenticated session's typed data access through the native lifecycle owned by
Architecture, not through an HTTP record endpoint or a new mailbox.

## Reads and subscriptions

All queries are constrained natively to the authenticated instance and user.
Caller-supplied owner IDs, project labels and computer Connected flags are not
authority. The native session/generation identifies every result, subscription
and acknowledgement; a result from the previous generation is discarded.

| UI purpose | Existing/additive collection | Required scope and fields |
| --- | --- | --- |
| Project picker | workjet_projects | Authorized active/archive projects; id, name, status. One active project in UI. |
| Physical checkout resolution | workjet_working_copies, workjet_computers | Existing logical project to working-copy/computer mapping and assigned status. Preserve current matcher. |
| Default group and personal chat rows | workjet_project_chats | Selected project; id/thread_id, kind, initial, worker_profile_id, owner and deletion state. No transcript in these rows. |
| Group worker membership | workjet_project_workers | Selected project; worker_profile_id, group_chat_id, active/removed status. |
| Worker identity and appearance link | workjet_worker_profile_bindings | Authenticated owner; worker_profile_id, assigned computer_id, optional crew_member_id, active/inactive state. |
| Existing Wesen appearance | ctox_crew_members | Existing public field projection only. Native Crew id stays distinct from Workjet profile id. |
| Conversation title/history | user_threads, user_thread_messages | Only selected authorized native thread; stable ordering, bounded history page and existing continuation. |
| Existing code-session association | workjet_sessions, workjet_session_transfers | Existing project/working-copy/computer, thread_id/coding_session_id and transfer/fence state; explicit mapping required. |
| Command outcomes | business_commands | Only pending command IDs and their authoritative terminal outcomes; preserve uncertain outcome. |
| Subtle worker activity | Existing mailbox/execution evidence | Bound to instance/project and participating stable profile IDs; distinguish transient status from human questions/errors. Exact producer binding still open. |

Initial snapshots and later changes must be coherent within one native session.
Subscriptions include updates, removals and revocation, not only inserts.
Switching instance or project disposes old subscriptions and active view data.
A stale asynchronous response must never switch the selected project back.
Revoked/private data is removed from active client visibility and authorized
subscription delivery stops. Durable source history is not deleted merely
because the current actor loses access.

A project list may be shown inside the picker; the left chat sidebar displays
only the selected project. It contains one group followed by per-worker groups
of separately named personal chats. Grouping uses worker_profile_id, never
name/model/harness inference. Existing unbound coding histories remain reachable
without inventing worker assignments or copying their messages.

## Commands and UI actions

Use the existing authenticated business command producer and its durable IDs.
The six additive native commands below are implemented in draft PR80.

| User action | Command | Body / acknowledgement |
| --- | --- | --- |
| Open an older project lacking its group | ctox.workjet.project.chat.ensure | project_id → group_chat_id |
| Add worker | ctox.workjet.project.worker.add | project_id, worker_profile_id → group/membership/first_chat IDs |
| Remove worker from project | ctox.workjet.project.worker.remove | Same identity; removes membership, preserves chats/history |
| Start another personal chat | ctox.workjet.project.chat.create | project_id, worker_profile_id, title; new explicit command ID → distinct chat_id |
| Register saved profile or choose existing Wesen | ctox.workjet.worker_profile.bind | Saved worker_profile_id, assigned computer_id, optional crew_member_id |
| Retire profile association | ctox.workjet.worker_profile.unbind | worker_profile_id; existing history remains |

New project upsert already ensures its default group. Concurrent add/ensure
replays retain one first chat; an explicit new personal chat receives a new
command identity. The UI exposes busy/failed/unknown outcomes and retains drafts;
it never translates a timeout into another operation with a new ID.
No group/profile action starts a worker execution or provisions a VM.

Conversation messages and human input must use the existing message/execution
commands for the bound conversation. Before wiring Send, explicitly resolve how
the native user_threads chat relates to the existing Workjet coding thread and
provider session. The presence of workjet_sessions.thread_id/coding_session_id
does not prove that this product mapping exists. Keep code events, files,
branches, diffs, approvals and provider handoff on their existing authority;
do not create a second transcript or silently reinterpret a native chat ID as
an unrelated Workjet Code thread ID. Group participant delivery and worker-to-
worker events also need an authoritative mapping to the existing mailbox.

## Authoritative profile save sequence

Existing Workjet source:
- WorkjetWorkerEditor preserves WorkjetWorkerProfile.id and assigns UUIDs only
  to newly created profiles.
- WorkerProfilePopupEditor updates configuration.workerProfiles keyed by id
  through serverEnvironment.updateSettings.
- apps/server/src/ws.ts:1736 delegates serverUpdateSettings to ServerSettingsService.
- apps/server/src/serverSettings.ts:611 serializes, validates/applies/normalizes,
  atomically persists settings, then changes cache/emits the acknowledged value.
- ChatComposer.tsx:918 stores selection in composerDraft.workjetWorkerId;
  availableProjects.ts:182 resolves that profile's computer for project scope.
  This is draft selection, not proof of a native conversation binding.

Profile binding must be derived from the acknowledged persisted profile, within
the verified active instance/computer scope. An unsaved editor draft cannot be
registered as an existing remote profile. Do not send credentials or private
Crew memory with this reference.

The Workjet settings write and native binding are separate existing authorities.
Their combined operation is not atomic. If settings save succeeds and binding
does not, show the saved settings plus pending/failed association and retain a
retryable intent using the existing durable command lifecycle. Do not silently
undo settings or claim a usable native binding. Reconcile profile removal and
computer reassignment explicitly; a display-name/model edit preserves identity.
A binding does not itself grant execution, device membership or VM control.

## Required observable acceptance

Desktop, web and mobile use the same scoped data semantics. Verify one default
group under concurrent peers; add/remove/re-add; multiple independent chats for
one worker; profile rename without regrouping; old history retention; unknown
command outcomes and retry; project/instance switches with delayed replies;
reconnect, reload and restart; authorization/revocation on fresh and existing
subscriptions; original provider handoff/code tools; no private transcript
copied into group content. Measure real query/replication pages and command
latency, and record the actual native/Workjet revisions.

The transient worker feed must be coalesced, non-scrolling and absent when idle;
opening its history is explicit. Human questions/failures remain discoverable.
VM stays an optional tool with a conditional right surface, using existing
native lifecycle/P2P streaming and distinct execution/takeover authority.

## Ownership

Architecture supplies the production local data/session connection and owns
transport, Sync/RxDB lifecycle and execution authority. This UX task owns the
released chat/profile domain, UI grouping/editors, scoped privacy and consumer
behavior once that boundary is available. Computer/SSH setup, migration,
Keychain and device release acceptance remain with the App task.
