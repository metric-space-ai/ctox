# Local Jour fixe transcript candidates

The signed macOS Workjet helper produces candidates, not native gateway speech
receipts. Workjet verifies its bundled helper and forwards final text through the
existing authenticated `business_commands` WebRTC path. No HTTP data bridge,
provider key, token rotation or new grant is needed.

`ctox.workjet.jour_fixe.transcript.local_candidate` consumes the generated
`LocalTranscriptCandidateRequest`:

- `operation_id`: stable per final submission, retained for retries.
- `request_id`: one final identity (helper request plus final sequence, bounded to
  128 characters); two operations cannot append the same final twice.
- `instance_id`: the authenticated native `biz_…` instance identity. Workjet's
  `managed:…` label is a UI alias and must not be substituted.
- `project_id`, `meeting_id`, `deck_revision`: exact current live scope.
- `expected_revision`: current native meeting revision, optimistic concurrency.
- `text`: exact UTF-8 final candidate, nonblank and at most 4096 bytes.

Native validates today's Owner/session/collection policy, the verified canonical
Owner alias, owned active project, registered Supervisor, native instance, live
meeting and current narrated deck. The check repeats inside the same writer
transaction as the text append and immutable domain receipt. Native generates
turn ID, sequence and persistence time. The stored turn has speaker `owner`,
modality `text`, and no audio, provider, gateway stream or latency claim.

The completed command returns the existing `MeetingMutationReceipt` plus
`local_candidate: LocalTranscriptCandidateReceipt`. The latter binds Owner,
instance, project, meeting, deck, helper final/request, operation, native turn ID,
sequence, revision, UTF-8 text SHA256 and persistence time. Its provenance is
`authenticated_owner_local_candidate` and **`provider_verified` is false**.
This proves exact authorized storage, not that CTOX observed an Apple/provider
speech execution. Consumers must explicitly check these two provenance fields;
structural DTO validation alone does not establish provider verification.

Workjet calls `onCommitted` only after this native command is completed and its
exact request/scope/text hash and typed receipt match the pending final. Partials
never enter this command. Room close, account/instance change and deck change
cancel the desktop stream and reject late callbacks. Native receipt replay also
rechecks current Owner/project/Supervisor/live deck; revoked or stale scopes
cannot recover an old receipt. The original operation returns its original
receipt without a second transcript turn; changed intent or another operation
for the same final is rejected. Domain receipt failures roll back both append
and local-final consumption.

This endpoint does not make an unprepared meeting ready. Starting a meeting
still needs a published deck with narration. Local helper narration publication
is a separate execution/persistence boundary; it must not manufacture gateway
`AudioRef` or relax readiness. Native gateway STT remains on its private
`VerifiedTranscriptFinal` path and uses modality `speech`.

The single fixture in `src/core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json`
generates both native and browser contracts. Desktop bridge/room wiring and
installed signed-helper acceptance belong to Workjet Main/Models.
