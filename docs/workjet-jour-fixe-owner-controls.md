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
state. Runtime metadata writes are bounded to1MiB. Preparation and a ready
state are not audio-file authorization; playback still uses the authorized
file/chunk path.

Comment delivery, bound-supervisor deck/proposal publication, registered audio
and speech, and confirmed to-dos becoming the Core supervisor goal remain
separate required delivery work. This slice and isolated source regressions do
not establish installed meeting acceptance.
