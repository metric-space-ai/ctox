# Workjet project chats (draft native contract)

This additive contract supplies the default project group, project workers and
private user-to-worker conversations. Message history stays in the existing
`user_threads` and `user_thread_messages` collections. The existing authenticated
business command dispatcher, Business OS store and RxDB projection remain the
data path.

## Commands and identity

All commands require the authorized native session owner; an owner supplied in
the payload is rejected. All payloads may carry the existing inbound_channel.

| Command | Payload | Result |
| --- | --- | --- |
| ctox.workjet.project.chat.ensure | project_id | group_chat_id |
| ctox.workjet.project.worker.add | project_id, worker_profile_id | group_chat_id, membership_id, first_chat_id |
| ctox.workjet.project.worker.remove | project_id, worker_profile_id | membership_id when present; history remains |
| ctox.workjet.project.chat.create | project_id, worker_profile_id, title | distinct chat_id per explicit command ID |
| ctox.workjet.worker_profile.bind | worker_profile_id, computer_id, optional crew_member_id | binding_id |
| ctox.workjet.worker_profile.unbind | worker_profile_id | inactive binding_id |

Project upsert creates its default group in the same business-record transaction.
The ensure command covers projects created before this contract. Group IDs and
first-chat IDs hash length-delimited owner/project/profile components. Explicit
additional chats include a separate discriminator and command ID. A retry
preserves identity; an unrelated existing history is never adopted.

The additional native-owned collections are workjet_project_chats,
workjet_project_workers and workjet_worker_profile_bindings. They contain
relationships, not transcripts. Worker removal or profile unbinding preserves
existing histories and refuses new work through an inactive membership/binding.

Workjet's producer is WorkjetWorkerEditor: editing preserves WorkjetWorkerProfile.id;
new profiles receive a UUID. Saving uses the branded WorkjetWorkerProfileId and
the existing serverEnvironment.updateSettings workerProfiles array keyed by id.
A binding registers an owner-authorized reference to this identity and an
already assigned computer. It does not independently discover a remote profile,
copy profile settings, create Crew, provision a VM or grant execution authority.
The Workjet producer must still integrate registration after its authoritative
profile save. Crew references are checked against the existing native Crew
reader, including archived/unreadable members.

## Privacy and transaction boundaries

Relationships are resolved from the native store. Replication and MCP query,
get, related activity and command-status reads apply an additional constraint
before the existing administrator shortcut. Private execution results follow
the existing run/event-to-queue-to-command references. Generic Threads commands
cannot add another human participant to these conversations.

The project, membership and first-chat records commit atomically in the existing
SQLite business store. RxDB projections are published afterward using the
existing writer. This is not a cross-store atomic commit or a new outbox.
Projection recovery and real peer visibility still require acceptance evidence.
New execution producers must retain their direct thread reference; an absent
legacy relation is not proof of a new private execution's visibility.

## Integration and validation

The implementation is based on CTOX 7198613f567e3cef2094ac12eebc713c5b9306e4
and targets codex/native-transport-parity. Architecture owns transport, lifecycle
and shared integration. Shared diffs are limited to command registration,
project upsert, scoped Threads/MCP guards, generated schemas and cache revision.

Canonical module/schema/hash generators were run. Besides the new CTOX/reports
schemas, the module generator repairs existing Threads ticket_key JSON drift
already present in schema.js. No existing collection version was changed.
The browser bundle must match the generated source; the focused CI retains its
pinned-esbuild artifact and fails until the matching bundle is committed.

Native regression source covers concurrent first-chat creation, explicit-chat
replay, transaction rollback, history preservation, profile/computer ownership,
existing Crew, MCP privacy, replication revocation and related private results.
These tests are not yet a successful execution claim. Local compilation is
blocked by the shared host capacity gate; the focused hosted CI compiles and
runs the actual native modules.

Still required: Workjet producers and consumers, real two-client concurrency and
revocation, measured command timings, complete cache graph validation, full
native/browser regression checks, actual Desktop/mobile UI evidence, private
worker activity presentation and the generic VM tool/stream/takeover workflow.
No production or installed-app acceptance is claimed.
