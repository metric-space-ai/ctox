# Verified native meeting transcript consumption

This native adapter composes the opaque speech producer receipt with the current
live meeting binding. It does not introduce a browser endpoint or a credential.
The existing native transport must authenticate its peer/session before each
operation, check collection permission, and revalidate after awaited provider I/O.

After opening a real `SpeechGateway` stream, call
`project_chats::jour_fixe_speech::BoundTranscription::bind(root, capability_token,
live_binding, stream)` before accepting PCM. The wrapper owns that non-cloneable
stream. Its `revalidate` rereads the signed actor/grants and current live meeting;
`stream_mut` exposes the existing bounded append/finish/verified-event methods.
Drop or `cancel` retires the gateway task. The transport must do so on leave,
revocation, scope change, timeout or a terminal/review meeting transition.

On an actual `VerifiedTranscriptEvent::Final`, pass the opaque final to
`bound.stage_final(root, token, final)`. It must be from the same native stream.
No caller-deserialized transcript can create this private receipt. The receipt
binds current Owner/project/meeting/Supervisor/deck and records actual producer
sequence, model and timing. PCM, tokens and credentials are never persisted.

`submit_staged_final(root, token, bound.binding(), bound.stream_id())` admits the
native turn through the ordinary **ReplicatedPeer** command path. The transcript,
receipt consumption, operation replay and domain application receipt commit or
roll back together. A bounded caller retry uses the saved producer final, never
another STT request. A consumed final keeps its original command/intent. Meeting
revision, transcript sequence and stream consumption are checked under the same
writer. Wire speech text without an exact staged native final remains rejected.

The saved turn is Owner speech with a real stream ID. It does not identify a
human voice or fabricate a provider run ID. Gateway finish-to-final timing is
kept private and never presented as client sentence-end latency.

This slice is native provenance/persistence. Browser PCM transport, narration
file publication, microphone capture cleanup and installed end-to-end latency
remain explicit follow-up work. Transport-fixture tests are not provider or
product acceptance.
