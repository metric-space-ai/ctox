import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import test from 'node:test';
import {
  readJourFixePresentationContent, presentationCanvasSavePayload, presentationMutationFromResult,
  presentationReadPayload, PRESENTATION_RANGE_MAX_BYTES,
} from './jour-fixe-presentation.mjs';
import { sha256Hex } from './file-integrity.js';
globalThis.crypto ??= webcrypto;

async function fixture() {
  const bytes = new TextEncoder().encode(JSON.stringify({ schemaVersion: 'learnordie.slide.v1', title: 'Regeltermin' }));
  const sha = await sha256Hex(bytes);
  const manifest = {
    presentation_id: 'workjet_presentation_1', project_id: 'project', meeting_id: 'meeting', owner_user_id: 'owner',
    title: 'Regeltermin', revision: 3, document_schema: 'learnordie.slide.v1', document_file_id: 'deck-file',
    document_generation_id: 'deck-generation', document_sha256: sha, document_bytes: bytes.length,
    slide_ids: ['s1'], source: 'owner', updated_by: 'owner', updated_at_ms: 1,
  };
  const file = { id: 'deck-file', owner_id: 'owner', linked_collection: 'workjet_presentations',
    linked_record_id: 'workjet_presentation_1', kind: 'file', content_state: 'available', mime_type: 'application/json',
    content_hash_scheme: 'sha256-bytes-v1', content_hash: sha, content_generation_id: 'deck-generation',
    size_bytes: bytes.length };
  const request = { action: 'project.presentation.content.read', commandId: 'command', projectId: 'project',
    meetingId: 'meeting', presentationId: 'workjet_presentation_1', revision: 3, offset: 0, length: 4096 };
  let reads = 0;
  const deps = {
    assertCurrent() {},
    async readManifest() { return structuredClone(manifest); },
    async readMetadata() { return structuredClone(file); },
    async readRange(id, range) {
      assert.equal(id, 'deck-file'); reads++;
      const part = bytes.subarray(range.offset, range.offset + range.length);
      return [{ sequence: 0, bytesBase64: Buffer.from(part).toString('base64'), hash: await sha256Hex(part) }];
    },
  };
  return { bytes, manifest, file, request, deps, reads: () => reads };
}

test('returns one bounded range of the current revision with its stored hash', async () => {
  const f = await fixture();
  const r = await readJourFixePresentationContent({ ...f.request, offset: 2, length: 10 }, f.deps);
  assert.equal(r.range.offset, 2); assert.equal(r.range.length, 10); assert.equal(r.range.total_bytes, f.bytes.length);
  assert.equal(r.range.revision, 3); assert.equal(r.range.document_sha256, f.manifest.document_sha256);
  assert.deepEqual(Buffer.from(r.range.data_base64, 'base64'), Buffer.from(f.bytes.subarray(2, 12)));
});

test('a newer revision or a different file is never served for an old request', async () => {
  const f = await fixture();
  await assert.rejects(readJourFixePresentationContent({ ...f.request, revision: 2 }, f.deps), { code: 'PRESENTATION_SCOPE_CHANGED' });
  const g = await fixture();
  g.deps.readMetadata = async () => ({ ...g.file, content_generation_id: 'other' });
  await assert.rejects(readJourFixePresentationContent(g.request, g.deps), { code: 'PRESENTATION_INTEGRITY_FAILED' });
  assert.equal(f.reads() + g.reads(), 0);
});

test('ranges above the Workjet response budget and foreign keys are rejected before reading', async () => {
  for (const change of [{ length: PRESENTATION_RANGE_MAX_BYTES + 1 }, { offset: -1 }, { fileId: 'foreign' }]) {
    const f = await fixture();
    await assert.rejects(readJourFixePresentationContent({ ...f.request, ...change }, f.deps), { code: 'PRESENTATION_INVALID_REQUEST' });
    assert.equal(f.reads(), 0);
  }
});

test('read and save payloads follow the shared wire contract', () => {
  assert.deepEqual(presentationReadPayload({ action: 'project.presentation.read', commandId: 'c', projectId: 'p', meetingId: 'm' }),
    { project_id: 'p', meeting_id: 'm' });
  const save = { action: 'project.presentation.canvas.save', commandId: 'c', projectId: 'p', meetingId: 'm',
    operationId: 'op', presentationId: 'pres', expectedRevision: 3, slideId: 's1', sceneJson: '{"elements":[]}' };
  assert.equal(presentationCanvasSavePayload(save).expected_revision, 3);
  assert.throws(() => presentationCanvasSavePayload({ ...save, sceneJson: '[]' }), { code: 'PRESENTATION_INVALID_REQUEST' });
  assert.throws(() => presentationCanvasSavePayload({ ...save, expectedRevision: 0 }), { code: 'PRESENTATION_INVALID_REQUEST' });
});

test('a save receipt must answer exactly the submitted operation and revision', async () => {
  const f = await fixture();
  const payload = { operation_id: 'op', presentation_id: 'workjet_presentation_1', expected_revision: 2, slide_id: 's1', scene_json: '{}' };
  const mutation = { operation_id: 'op', presentation_id: 'workjet_presentation_1', project_id: 'project', meeting_id: 'meeting',
    revision: 3, document_sha256: f.manifest.document_sha256, document_bytes: f.bytes.length, slide_ids: ['s1'] };
  const scope = { projectId: 'project', meetingId: 'meeting' };
  const ok = presentationMutationFromResult({ ok: true, contract: 'ctox.workjet.presentation.v1', mutation, presentation: f.manifest }, payload, scope);
  assert.equal(ok.presentation.revision, 3);
  assert.throws(() => presentationMutationFromResult({ ok: true, contract: 'ctox.workjet.presentation.v1',
    mutation: { ...mutation, revision: 4 }, presentation: f.manifest }, payload, scope), { code: 'PRESENTATION_INTEGRITY_FAILED' });
});
