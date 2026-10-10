import { readStoredFileFromDemandChunks, sha256Hex } from './file-integrity.js';

export const NARRATION_READ_ACTION = 'project.jour_fixe.narration.read';
export const NARRATION_RANGE_MAX_BYTES = 256 * 1024;
export const NARRATION_MAX_BYTES = 8 * 1024 * 1024;
const HASH = /^[a-f0-9]{64}$/;
const keys = new Set(['action', 'commandId', 'projectId', 'meetingId', 'deckRevision', 'slideId', 'offset', 'length']);
function fail(code, message) { throw Object.assign(new Error(message), { code }); }
function id(value) {
  return typeof value === 'string' && value === value.trim() && value.length > 0
    && value.length <= 128 && !/[\u0000-\u001f\u007f]/.test(value);
}
export function validateNarrationRead(request) {
  if (!request || typeof request !== 'object' || Array.isArray(request)
    || Object.keys(request).some(key => !keys.has(key))
    || request.action !== NARRATION_READ_ACTION
    || !['commandId', 'projectId', 'meetingId', 'slideId'].every(key => id(request[key]))
    || !Number.isSafeInteger(request.deckRevision) || request.deckRevision < 1
    || !Number.isSafeInteger(request.offset) || request.offset < 0
    || !Number.isSafeInteger(request.length) || request.length < 1
    || request.length > NARRATION_RANGE_MAX_BYTES) {
    fail('NARRATION_INVALID_REQUEST', 'Invalid narration scope or bounded byte range.');
  }
}
function sameFile(file, meeting, audio) {
  if (!file || file._deleted === true || file.is_deleted === true
    || file.id !== audio.file_id || file.owner_id !== meeting.owner_user_id
    || file.linked_collection !== 'workjet_jour_fixe_meetings' || file.linked_record_id !== meeting.id
    || file.kind !== 'file' || file.content_state !== 'available' || file.mime_type !== 'audio/wav'
    || file.content_hash_scheme !== 'sha256-bytes-v1' || file.content_hash !== audio.sha256
    || file.content_generation_id !== audio.generation_id
    || !Number.isSafeInteger(file.size_bytes) || file.size_bytes < 1 || file.size_bytes > NARRATION_MAX_BYTES) {
    fail('NARRATION_INTEGRITY_FAILED', 'Narration file does not match the retained meeting audio.');
  }
}
function encode(bytes) {
  let text = '';
  for (let offset = 0; offset < bytes.length; offset += 16_384) {
    text += String.fromCharCode(...bytes.subarray(offset, offset + 16_384));
  }
  return btoa(text);
}
/** Native meeting receipt + policy-checked rxdb.file.fetch; no caller-selected file or synthesis. */
export async function readJourFixeNarration(request, { readMeeting, readMetadata, readRange, assertCurrent }) {
  validateNarrationRead(request);
  const captured = Object.freeze({ ...request });
  assertCurrent();
  const meeting = await readMeeting(captured);
  assertCurrent();
  if (!meeting || meeting.id !== captured.meetingId || meeting.project_id !== captured.projectId
    || meeting.deck_revision !== captured.deckRevision || !Number.isSafeInteger(meeting.revision)
    || ['failed', 'cancelled'].includes(meeting.state)) {
    fail('NARRATION_SCOPE_CHANGED', 'The requested meeting or deck is no longer current.');
  }
  const slide = meeting.slides?.find(item => item.id === captured.slideId && item.meeting_id === meeting.id);
  const audio = slide?.audio;
  if (!audio) fail('NARRATION_NOT_READY', 'This slide has no retained narration audio.');
  if (!id(audio.file_id) || !id(audio.generation_id) || audio.mime_type !== 'audio/wav'
    || audio.format !== 'wav' || !HASH.test(audio.sha256) || !HASH.test(audio.narration_text_sha256)
    || !['native_gateway', 'authenticated_owner_local_audio'].includes(audio.provenance)
    || !Number.isSafeInteger(audio.duration_ms) || audio.duration_ms <= 0 || audio.duration_ms > 300_000
    || typeof slide.body_markdown !== 'string' || new TextEncoder().encode(slide.body_markdown).length > 4096
    || await sha256Hex(slide.body_markdown) !== audio.narration_text_sha256) {
    fail('NARRATION_INTEGRITY_FAILED', 'The retained narration does not match this slide.');
  }
  assertCurrent();
  const before = await readMetadata(audio.file_id);
  assertCurrent();
  sameFile(before, meeting, audio);
  if (captured.offset >= before.size_bytes) fail('NARRATION_INVALID_REQUEST', 'Narration byte offset is outside the file.');
  const length = Math.min(captured.length, before.size_bytes - captured.offset);
  const chunks = await readRange(audio.file_id, { offset: captured.offset, length });
  assertCurrent();
  if (!Array.isArray(chunks) || chunks.length > 512
    || chunks.some(chunk => typeof chunk?.bytesBase64 !== 'string' || !HASH.test(chunk.hash)
      || chunk.bytesBase64.length > 4 * Math.ceil(length / 3))
    || chunks.reduce((size, chunk) => size + chunk.bytesBase64.length, 0) > 4 * Math.ceil(length / 3) + 2048) {
    fail('NARRATION_INTEGRITY_FAILED', 'Narration byte stream exceeded its range or lacks integrity metadata.');
  }
  const blob = await readStoredFileFromDemandChunks(chunks, 'audio/wav');
  if (blob.size !== length) fail('NARRATION_INTEGRITY_FAILED', 'Narration byte stream is incomplete.');
  // Re-read with a fresh native demand token; cached metadata cannot confirm retention.
  const after = await readMetadata(audio.file_id);
  assertCurrent();
  sameFile(after, meeting, audio);
  if (after.size_bytes !== before.size_bytes) fail('NARRATION_INTEGRITY_FAILED', 'Narration file size changed.');
  const current = await readMeeting(captured);
  assertCurrent();
  const currentSlide = current?.slides?.find(item => item.id === captured.slideId && item.meeting_id === captured.meetingId);
  if (current?.id !== meeting.id || current.project_id !== meeting.project_id
    || current.owner_user_id !== meeting.owner_user_id || current.deck_revision !== meeting.deck_revision
    || ['failed', 'cancelled'].includes(current.state)
    || currentSlide?.body_markdown !== slide.body_markdown
    || currentSlide?.audio?.file_id !== audio.file_id
    || currentSlide.audio.generation_id !== audio.generation_id
    || currentSlide.audio.sha256 !== audio.sha256
    || currentSlide.audio.narration_text_sha256 !== audio.narration_text_sha256) {
    fail('NARRATION_SCOPE_CHANGED', 'The meeting narration changed during the read.');
  }
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const rangeSha256 = await sha256Hex(bytes);
  assertCurrent();
  return {
    action: NARRATION_READ_ACTION, commandId: captured.commandId,
    projectId: captured.projectId, meetingId: captured.meetingId, deckRevision: captured.deckRevision,
    slideId: captured.slideId, meetingRevision: current.revision, audio: JSON.parse(JSON.stringify(audio)),
    totalBytes: before.size_bytes, offset: captured.offset, length,
    bytesBase64: encode(bytes), rangeSha256,
  };
}
