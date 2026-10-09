import { readStoredFileFromDemandChunks, sha256Hex } from './file-integrity.js';
import { validatePresentationValue } from './workjet-presentation-contract.generated.mjs';

// Workjet reads and saves Jour fixe presentations through these three actions.
// Reads correlate the native manifest with the policy-checked rxdb.file.fetch
// path; saves are owner commands. No caller-selected file, no HTTP data path.
export const PRESENTATION_READ_ACTION = 'project.presentation.read';
export const PRESENTATION_CONTENT_READ_ACTION = 'project.presentation.content.read';
export const PRESENTATION_CANVAS_SAVE_ACTION = 'project.presentation.canvas.save';
export const PRESENTATION_ACTIONS = Object.freeze([
  PRESENTATION_READ_ACTION, PRESENTATION_CONTENT_READ_ACTION, PRESENTATION_CANVAS_SAVE_ACTION,
]);
export const PRESENTATION_RANGE_MAX_BYTES = 128 * 1024;
export const PRESENTATION_MAX_BYTES = 8 * 1024 * 1024;
const HASH = /^[a-f0-9]{64}$/;

function fail(code, message) { throw Object.assign(new Error(message), { code }); }
function id(value) {
  return typeof value === 'string' && value === value.trim() && value.length > 0
    && value.length <= 128 && !/[\u0000-\u001f\u007f]/.test(value);
}
function exactKeys(request, keys) {
  if (!request || typeof request !== 'object' || Array.isArray(request)
    || Object.keys(request).some(key => !keys.includes(key))) {
    fail('PRESENTATION_INVALID_REQUEST', 'Invalid presentation request.');
  }
}

/** Command payload for `ctox.workjet.presentation.read`. */
export function presentationReadPayload(request) {
  exactKeys(request, ['action', 'commandId', 'projectId', 'meetingId']);
  if (request.action !== PRESENTATION_READ_ACTION
    || !['commandId', 'projectId', 'meetingId'].every(key => id(request[key]))) {
    fail('PRESENTATION_INVALID_REQUEST', 'Invalid presentation read scope.');
  }
  const payload = { project_id: request.projectId, meeting_id: request.meetingId };
  const validation = validatePresentationValue('ReadPresentationRequest', payload);
  if (validation.ok !== true) fail('PRESENTATION_INVALID_REQUEST', validation.error);
  return payload;
}

/** Command payload for `ctox.workjet.presentation.canvas.save`. */
export function presentationCanvasSavePayload(request) {
  exactKeys(request, ['action', 'commandId', 'projectId', 'meetingId', 'operationId', 'presentationId',
    'expectedRevision', 'slideId', 'sceneJson']);
  if (request.action !== PRESENTATION_CANVAS_SAVE_ACTION
    || !['commandId', 'projectId', 'meetingId', 'operationId', 'presentationId'].every(key => id(request[key]))) {
    fail('PRESENTATION_INVALID_REQUEST', 'Invalid presentation save scope.');
  }
  const payload = {
    operation_id: request.operationId, presentation_id: request.presentationId,
    expected_revision: request.expectedRevision, slide_id: request.slideId, scene_json: request.sceneJson,
  };
  const validation = validatePresentationValue('SavePresentationCanvasRequest', payload);
  if (validation.ok !== true) fail('PRESENTATION_INVALID_REQUEST', validation.error);
  try {
    const scene = JSON.parse(payload.scene_json);
    if (!scene || typeof scene !== 'object' || Array.isArray(scene)) throw new Error('not an object');
  } catch {
    fail('PRESENTATION_INVALID_REQUEST', 'The canvas scene must be a JSON object.');
  }
  return payload;
}

/** Verifies a read receipt result and returns the manifest or null. */
export function presentationFromReadResult(result, scope) {
  const validation = validatePresentationValue('ReadPresentationResponse',
    { contract: result?.contract, ...(result?.presentation ? { presentation: result.presentation } : {}) });
  if (result?.ok !== true || validation.ok !== true) {
    fail('PRESENTATION_INTEGRITY_FAILED', validation.error || 'Presentation read was not confirmed.');
  }
  const manifest = result.presentation ?? null;
  if (manifest && (manifest.project_id !== scope.projectId || manifest.meeting_id !== scope.meetingId)) {
    fail('PRESENTATION_SCOPE_CHANGED', 'The presentation belongs to another meeting.');
  }
  return manifest;
}

/** Verifies a save receipt result against the request it answers. */
export function presentationMutationFromResult(result, payload, scope) {
  const mutation = result?.mutation;
  const validation = validatePresentationValue('PresentationMutationReceipt', mutation);
  if (result?.ok !== true || validation.ok !== true) {
    fail('PRESENTATION_INTEGRITY_FAILED', validation.error || 'Presentation save was not confirmed.');
  }
  if (mutation.operation_id !== payload.operation_id || mutation.presentation_id !== payload.presentation_id
    || mutation.project_id !== scope.projectId || mutation.meeting_id !== scope.meetingId
    || mutation.revision !== payload.expected_revision + 1) {
    fail('PRESENTATION_INTEGRITY_FAILED', 'The save receipt does not answer this request.');
  }
  return { mutation, presentation: presentationFromReadResult(
    { ok: true, contract: result.contract, presentation: result.presentation }, scope) };
}

function validateContentRead(request) {
  exactKeys(request, ['action', 'commandId', 'projectId', 'meetingId', 'presentationId', 'revision', 'offset', 'length']);
  if (request.action !== PRESENTATION_CONTENT_READ_ACTION
    || !['commandId', 'projectId', 'meetingId', 'presentationId'].every(key => id(request[key]))
    || !Number.isSafeInteger(request.revision) || request.revision < 1
    || !Number.isSafeInteger(request.offset) || request.offset < 0
    || !Number.isSafeInteger(request.length) || request.length < 1
    || request.length > PRESENTATION_RANGE_MAX_BYTES) {
    fail('PRESENTATION_INVALID_REQUEST', 'Invalid presentation scope or bounded byte range.');
  }
}

function sameFile(file, manifest) {
  if (!file || file._deleted === true || file.is_deleted === true
    || file.id !== manifest.document_file_id || file.owner_id !== manifest.owner_user_id
    || file.linked_collection !== 'workjet_presentations' || file.linked_record_id !== manifest.presentation_id
    || file.kind !== 'file' || file.content_state !== 'available' || file.mime_type !== 'application/json'
    || file.content_hash_scheme !== 'sha256-bytes-v1' || file.content_hash !== manifest.document_sha256
    || file.content_generation_id !== manifest.document_generation_id
    || file.size_bytes !== manifest.document_bytes || file.size_bytes > PRESENTATION_MAX_BYTES) {
    fail('PRESENTATION_INTEGRITY_FAILED', 'The presentation file does not match its manifest.');
  }
}

function encode(bytes) {
  let text = '';
  for (let offset = 0; offset < bytes.length; offset += 16_384) {
    text += String.fromCharCode(...bytes.subarray(offset, offset + 16_384));
  }
  return btoa(text);
}

/** One bounded, integrity-checked byte range of a stored presentation revision. */
export async function readJourFixePresentationContent(request, { readManifest, readMetadata, readRange, assertCurrent }) {
  validateContentRead(request);
  const captured = Object.freeze({ ...request });
  assertCurrent();
  const manifest = await readManifest(captured);
  assertCurrent();
  if (!manifest || manifest.presentation_id !== captured.presentationId
    || manifest.project_id !== captured.projectId || manifest.meeting_id !== captured.meetingId) {
    fail('PRESENTATION_SCOPE_CHANGED', 'The requested presentation is no longer current.');
  }
  if (manifest.revision !== captured.revision) {
    fail('PRESENTATION_SCOPE_CHANGED', 'The presentation has a newer revision.');
  }
  if (!HASH.test(manifest.document_sha256) || !id(manifest.document_file_id) || !id(manifest.document_generation_id)) {
    fail('PRESENTATION_INTEGRITY_FAILED', 'The presentation manifest is incomplete.');
  }
  const before = await readMetadata(manifest.document_file_id);
  assertCurrent();
  sameFile(before, manifest);
  if (captured.offset >= before.size_bytes) fail('PRESENTATION_INVALID_REQUEST', 'Presentation byte offset is outside the file.');
  const length = Math.min(captured.length, before.size_bytes - captured.offset);
  const chunks = await readRange(manifest.document_file_id, { offset: captured.offset, length });
  assertCurrent();
  if (!Array.isArray(chunks) || chunks.length > 512
    || chunks.some(chunk => typeof chunk?.bytesBase64 !== 'string' || !HASH.test(chunk.hash)
      || chunk.bytesBase64.length > 4 * Math.ceil(length / 3))
    || chunks.reduce((size, chunk) => size + chunk.bytesBase64.length, 0) > 4 * Math.ceil(length / 3) + 2048) {
    fail('PRESENTATION_INTEGRITY_FAILED', 'Presentation byte stream exceeded its range or lacks integrity metadata.');
  }
  const blob = await readStoredFileFromDemandChunks(chunks, 'application/json');
  if (blob.size !== length) fail('PRESENTATION_INTEGRITY_FAILED', 'Presentation byte stream is incomplete.');
  const after = await readMetadata(manifest.document_file_id);
  assertCurrent();
  sameFile(after, manifest);
  const bytes = new Uint8Array(await blob.arrayBuffer());
  const range = {
    presentation_id: manifest.presentation_id, revision: manifest.revision,
    offset: captured.offset, length, total_bytes: before.size_bytes,
    document_sha256: manifest.document_sha256, data_base64: encode(bytes),
  };
  const validation = validatePresentationValue('PresentationContentRange', range);
  if (validation.ok !== true) fail('PRESENTATION_INTEGRITY_FAILED', validation.error);
  return { action: PRESENTATION_CONTENT_READ_ACTION, commandId: captured.commandId,
    projectId: captured.projectId, meetingId: captured.meetingId, range, rangeSha256: await sha256Hex(bytes) };
}
