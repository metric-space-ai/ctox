# Retained Jour fixe narration playback

The Shell project-control port exposes `project.jour_fixe.narration.read`. It reads an existing slide narration; it never synthesizes audio or accepts a provider, model, voice, text, URL, path, credentials or caller-selected file.

Request:

```js
{
  action: 'project.jour_fixe.narration.read',
  commandId, projectId, meetingId, deckRevision, slideId,
  offset: 0, length: 262144
}
```

The paired Shell supplies its actual native instance and actor. Every range resolves the current meeting through the existing native `ctox.workjet.jour_fixe.meeting.read` command receipt, then demands fresh `desktop_files` metadata and bytes through `rxdb.file.fetch` on the authenticated primary WebRTC connection. Native collection/file policy remains authoritative. The file must belong to the meeting Owner, link to that meeting, carry the retained generation and SHA256, and be an available WAV of at most 8 MiB. The unchanged slide body must match the retained narration-text SHA256. Fresh metadata and another native meeting read after transfer reject generation, Owner, deck, slide or audio replacement. An increasing meeting revision caused by a transcript/comment does not invalidate unchanged narration.

Response:

```js
{
  action: 'project.jour_fixe.narration.read',
  commandId, projectId, meetingId, deckRevision, slideId, meetingRevision,
  audio, // retained AudioRef, including file_id, generation_id and both SHA256 values
  totalBytes, offset, length, bytesBase64, rangeSha256
}
```

Ranges are at most 256 KiB; the last range is clipped to the actual file size. An offset outside the file fails. Chunk sequence, chunk hashes and exact returned size are checked before returning bytes. The consumer must correlate every response, retain one exact file/generation/full-content/text-hash identity across ranges, verify each range SHA256 and the full audio SHA256, and only then create a local `Blob` URL. An AudioRef by itself is never a playable URL. Both native gateway narration and authenticated Owner local narration use this path; local provenance does not become provider-verified speech.

Each call has a 29-second total deadline. A changed paired session, database, sync peer/query generation or native instance discards the result. Consumer cancellation stops subsequent range requests and ignores the current result; the existing individual file fetch remains bounded by its native timeout. Do not call the shared loader's `abortAllInFlight`, which would cancel unrelated readers. On slide/scope/meeting transition or unmount, stop playback, discard incomplete bytes and revoke any owned Blob URL. No global collection or connection is stopped.

Errors distinguish `NARRATION_NOT_READY`, `NARRATION_INVALID_REQUEST`, `NARRATION_SCOPE_CHANGED` and `NARRATION_INTEGRITY_FAILED`; native policy/transport errors propagate without being renamed as credential failures.

Narration playback readiness is separate from microphone authorization. A stored verified narration may play while the room's microphone remains disabled. Installation and hardware/latency acceptance are separate from source tests.
