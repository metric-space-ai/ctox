# Owner meeting controls

The native command plane implements the owner-side edit slice below. These are
policy-checked workspace data writes, with the current canonical project owner
and registered supervisor rechecked in the same transaction as the metadata
and immutable domain application receipt. A meeting ID resolves the native
project; caller `record_id` cannot grant access. No HTTP data path is added.

All requests use `module: ctox`, the optional project `record_id`, and the
shared `ctox.workjet.jour_fixe.v1` fixture request types:

- `ctox.workjet.jour_fixe.meeting.start`: `MeetingTransitionRequest`; ready
  meeting with a published deck and narration references becomes live.
- `ctox.workjet.jour_fixe.meeting.end`: `MeetingTransitionRequest`; live becomes
  review. This does not confirm goals or completion.
- `ctox.workjet.jour_fixe.transcript.append`: the owner-text subset of
  `AppendTranscriptRequest`; live/review only, exact meeting and next sequence.
  This path rejects supervisor identity, speech modality, audio, STT run/stream
  and latency claims. Registered speech/supervisor production is a separate
  owned integration; storing owner text is not proof of a supervisor reply.
- `ctox.workjet.jour_fixe.comment.add`: `AddCommentRequest`; live/review only,
  an existing slide at the exact current deck revision, normalized pin and
  nonempty text. The native canonical Owner and server timestamp are stored;
  caller author/event assertions fail. Comment IDs cannot collide with another
  item of meeting evidence. This persists feedback but does not claim a
  Supervisor response or enqueue a model turn.
- `ctox.workjet.jour_fixe.todos.revise`: `ProposeTodosRequest`; review only,
  an existing proposed list and its exact next proposal revision. Evidence IDs
  refer only to this meeting's slides/comments/transcript. The result remains
  proposed and contains no confirmation or Core goal receipt.

Each request has `operation_id`, `meeting_id`, `expected_revision`. An exact
operation replay returns its original compact receipt without another edit;
changed intent, stale revision, foreign ownership or changed supervisor fails.
An error rolls back metadata, operation and domain receipt together.

Result: `{ok:true, contract:"ctox.workjet.jour_fixe.v1", mutation:<MeetingMutationReceipt>}`.
The generated receipt carries operation/meeting/project IDs, native revision
and state, plus `changed_id` or `todos_revision` when applicable. It does not
copy the full meeting. The existing authorized `meeting.read` obtains current
state. Runtime metadata writes are bounded to 1 MiB. Preparation and a ready
state are not audio-file authorization; playback still uses the authorized
file/chunk path.

Supervisor processing of comments, bound-supervisor deck/proposal publication, registered audio
and speech, and confirmed to-dos becoming the Core supervisor goal remain
separate required delivery work. Their declared command names fail terminally
until their handlers land; they cannot fall through into recursive model tasks.
This slice and isolated source regressions do
not establish installed meeting acceptance.

## Browser control

`workjetProjectControl` exposes the same five Owner actions as
`project.jour_fixe.meeting.start`, `project.jour_fixe.meeting.end`,
`project.jour_fixe.transcript.append`, `project.jour_fixe.todos.revise` and
`project.jour_fixe.comment.add`.
All take `commandId`, `projectId`, `operationId`, `meetingId` and
`expectedRevision`. Text append additionally takes the shared snake-case `turn`
DTO; todo revision takes `proposalRevision` and the shared `items` DTOs.
Comment add takes `commentId`, `slideId`, `deckRevision`, `x`, `y` and `text`.
Its returned `changed_id` must exactly match the requested comment ID.
The native payload uses `meeting_id` and routes through `record_id=projectId`;
project ownership is resolved from the stored meeting, never a caller assertion.

The browser returns `{action, commandId, projectId, contract, mutation}` only
after a completed receipt matches the command, project, whole nested request
intent, operation, meeting and next revision. The compact `mutation` follows
`MeetingMutationReceipt`. Identity/session/database replacement during the wait
fails closed. Re-read the meeting after an uncertain result and reuse the same
operation and intent when retrying; a new operation must use the current revision.
The isolated Node and Chromium regressions exercise the actual control source
against a controlled transport fixture; they do not establish installed native
or Workjet room acceptance.

## Native speech adapter metadata seam

`business_os::project_chats::jour_fixe_owner::check_live_meeting_for_authenticated_actor`
returns an opaque `LiveMeetingBinding` to native Business OS code. It checks the
current canonical project Owner, stored meeting, registered Supervisor, exact
nonzero deck revision, live state and available narrated deck. The caller must
first perform the existing fresh native peer/session/collection authorization;
this metadata check is not a credential, grant or substitute for that ingress.

`LiveMeetingBinding::revalidate(root, authenticated_actor)` repeats the read
after an awaited provider operation. Conversation writes may advance the meeting
revision; changing owner, Supervisor, deck or live state invalidates the binding.
`meeting_revision()` returns the fresh CAS revision for the next mutation.
The helper opens only a read-only DEFERRED snapshot, creates/migrates nothing,
and drops all database state before returning. It holds no issuer fence across
network or audio operations and cannot be deserialized from a browser payload.

Verified speech-final persistence and narration file receipts remain a separate
integration. `execution::speech::TranscriptEvent` currently derives Deserialize
and is not by itself proof of gateway provenance. The Models producer must expose
a native-only verified receipt; do not pass deserialized client events to the
Owner text append handler or invent speaker/model/audio evidence.
