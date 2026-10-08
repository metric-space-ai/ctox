import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import test from 'node:test';
import { readJourFixeNarration, NARRATION_RANGE_MAX_BYTES } from './jour-fixe-narration.mjs';
import { sha256Hex } from './file-integrity.js';
globalThis.crypto ??= webcrypto;

async function fixture() {
  const bytes = new TextEncoder().encode('RIFF-bounded-audio-fixture');
  const audio = {
    file_id: 'retained-file', generation_id: 'immutable-generation', mime_type: 'audio/wav', format: 'wav',
    sha256: await sha256Hex(bytes), narration_text_sha256: await sha256Hex('Slide text'),
    provenance: 'native_gateway', duration_ms: 1200, source_run_id: 'retained-run', model: 'fixture-model', synthesis_duration_ms: 50,
  };
  const meeting = { id: 'meeting', project_id: 'project', owner_user_id: 'owner',
    deck_revision: 4, revision: 9, state: 'live',
    slides: [{ id: 'slide', meeting_id: 'meeting', title: 'Slide', position: 0, body_markdown: 'Slide text', audio }] };
  const file = { id: audio.file_id, owner_id: 'owner', linked_collection: 'workjet_jour_fixe_meetings',
    linked_record_id: 'meeting', kind: 'file', content_state: 'available', mime_type: 'audio/wav',
    content_hash_scheme: 'sha256-bytes-v1', content_hash: audio.sha256,
    content_generation_id: audio.generation_id, size_bytes: bytes.length };
  const request = { action: 'project.jour_fixe.narration.read', commandId: 'command', projectId: 'project',
    meetingId: 'meeting', deckRevision: 4, slideId: 'slide', offset: 0, length: 256 };
  let active = true; let fetches = 0; let metadataReads = 0; let meetingReads = 0;
  const deps = {
    assertCurrent() { if (!active) throw Object.assign(new Error('Scope changed'), { code: 'NARRATION_SCOPE_CHANGED' }); },
    async readMeeting() { meetingReads++; return structuredClone(meeting); },
    async readMetadata() { metadataReads++; return structuredClone(file); },
    async readRange(id, range) {
      assert.equal(id, audio.file_id); fetches++;
      const part = bytes.subarray(range.offset, range.offset + range.length);
      return [{ sequence: 0, bytesBase64: Buffer.from(part).toString('base64'), hash: await sha256Hex(part) }];
    },
  };
  return { bytes, audio, meeting, file, request, deps, cancel: () => { active = false; },
    counts: () => ({ fetches, metadataReads, meetingReads }) };
}
test('returns one correlated bounded byte range, preserving stored hashes and generation', async () => {
  const f = await fixture();
  const r = await readJourFixeNarration({ ...f.request, offset: 3, length: 8 }, f.deps);
  assert.equal(r.offset, 3); assert.equal(r.length, 8); assert.equal(r.totalBytes, f.bytes.length);
  assert.equal(r.commandId, 'command'); assert.equal(r.deckRevision, 4);
  assert.deepEqual(Buffer.from(r.bytesBase64, 'base64'), Buffer.from(f.bytes.subarray(3, 11)));
  assert.equal(r.rangeSha256, await sha256Hex(f.bytes.subarray(3, 11)));
  assert.deepEqual(r.audio, f.audio);
  assert.deepEqual(f.counts(), { fetches: 1, metadataReads: 2, meetingReads: 2 });
});
test('local Owner narration is playable without asserting gateway verification', async () => {
  const f = await fixture(); f.audio.provenance = 'authenticated_owner_local_audio';
  const r = await readJourFixeNarration(f.request, f.deps);
  assert.equal(r.audio.provenance, 'authenticated_owner_local_audio');
  assert.equal(Object.hasOwn(r, 'providerVerified'), false);
});
test('invalid ranges and caller-selected credentials/text/files are rejected before any read', async () => {
  for (const change of [{ length: NARRATION_RANGE_MAX_BYTES + 1 }, { offset: -1 }, { length: 0 },
    { fileId: 'foreign' }, { ownerUserId: 'foreign' }, { text: 'Injected' }, { model: 'grok' }, { url: 'https://invalid' }]) {
    const f = await fixture();
    await assert.rejects(readJourFixeNarration({ ...f.request, ...change }, f.deps), { code: 'NARRATION_INVALID_REQUEST' });
    assert.deepEqual(f.counts(), { fetches: 0, metadataReads: 0, meetingReads: 0 });
  }
});
test('different project/deck/meeting never fetches file bytes', async () => {
  for (const change of [{ deck_revision: 5 }, { project_id: 'foreign' }, { id: 'foreign' }]) {
    const f = await fixture(); Object.assign(f.meeting, change);
    await assert.rejects(readJourFixeNarration(f.request, f.deps), { code: 'NARRATION_SCOPE_CHANGED' });
    assert.equal(f.counts().fetches, 0);
  }
});
test('missing audio is not reported as an authentication or provider error', async () => {
  const f = await fixture(); delete f.meeting.slides[0].audio;
  await assert.rejects(readJourFixeNarration(f.request, f.deps), { code: 'NARRATION_NOT_READY' });
});
test('foreign file Owner/link, hash, generation, excessive size or wrong MIME fails before file fetch', async () => {
  for (const change of [{ owner_id: 'foreign' }, { linked_record_id: 'foreign' },
    { content_hash: 'a'.repeat(64) }, { content_generation_id: 'new' },
    { size_bytes: 8 * 1024 * 1024 + 1 }, { mime_type: 'text/html' }]) {
    const f = await fixture(); Object.assign(f.file, change);
    await assert.rejects(readJourFixeNarration(f.request, f.deps), { code: 'NARRATION_INTEGRITY_FAILED' });
    assert.equal(f.counts().fetches, 0);
  }
});
test('narration text/hash mismatch and missing generation never fetch bytes', async () => {
  for (const change of [{ narration_text_sha256: 'b'.repeat(64) }, { generation_id: null }]) {
    const f = await fixture(); Object.assign(f.audio, change);
    await assert.rejects(readJourFixeNarration(f.request, f.deps), { code: 'NARRATION_INTEGRITY_FAILED' });
    assert.equal(f.counts().fetches, 0);
  }
});
test('changed metadata after the range discards bytes', async () => {
  const f = await fixture(); const read = f.deps.readRange;
  f.deps.readRange = async (...args) => { const chunks = await read(...args); f.file.content_generation_id = 'replaced'; return chunks; };
  await assert.rejects(readJourFixeNarration(f.request, f.deps), { code: 'NARRATION_INTEGRITY_FAILED' });
});
test('changed native meeting after the range discards bytes', async () => {
  const f = await fixture(); const read = f.deps.readRange;
  f.deps.readRange = async (...args) => { const chunks = await read(...args); f.meeting.deck_revision++; return chunks; };
  await assert.rejects(readJourFixeNarration(f.request, f.deps), { code: 'NARRATION_SCOPE_CHANGED' });
});
test('scope cancellation while reading discards the range', async () => {
  const f = await fixture(); const read = f.deps.readRange;
  f.deps.readRange = async (...args) => { const chunks = await read(...args); f.cancel(); return chunks; };
  await assert.rejects(readJourFixeNarration(f.request, f.deps), { code: 'NARRATION_SCOPE_CHANGED' });
});
test('damaged or missing chunk and short stream fail closed', async () => {
  for (const mutation of [
    chunks => [{ ...chunks[0], hash: 'c'.repeat(64) }],
    chunks => [{ ...chunks[0], sequence: 1 }],
    chunks => [{ ...chunks[0], bytesBase64: '', hash: undefined }],
    chunks => [{ ...chunks[0], bytesBase64: Buffer.from('short').toString('base64') }],
  ]) {
    const f = await fixture(); const read = f.deps.readRange;
    f.deps.readRange = async (...args) => mutation(await read(...args));
    await assert.rejects(readJourFixeNarration(f.request, f.deps));
  }
});


test('Shell narration read uses native meeting receipts, fresh file metadata and demand bytes', async () => {
  const { readFileSync } = await import('node:fs'); const vm = await import('node:vm');
  const { validateJourFixeValue, JOUR_FIXE_SCHEMA } = await import('./workjet-jour-fixe-contract.generated.mjs');
  const { validateNarrationRead } = await import('./jour-fixe-narration.mjs');
  const f = await fixture();
  const corpus = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json', import.meta.url)));
  const complete = structuredClone(corpus.valid_cases.find(item => item.type === 'Meeting').value);
  Object.assign(complete, f.meeting);
  const commands = []; const revisions = []; let generation = 'peer-generation';
  const peer = {
    collection: { demandLoader: {}, schema: { primaryPath: 'id' },
      find(query) { return { async exec() {
        assert.equal(query.selector.id.$eq, f.audio.file_id);
        assert.ok(query.requireRevision); assert.ok(query.signal); revisions.push(query.requireRevision);
        return [structuredClone(f.file)];
      } }; } },
    async awaitQueryReady() {}, collectionQueryGenerationToken: () => generation,
    demandFileLoader: { fetchFile: (fileId, { range }) => f.deps.readRange(fileId, range) },
  };
  const state = {
    session: { id: 'verified-owner-alias' }, db: { collection: () => ({}) },
    syncConfig: { instance_id: 'biz_actual-paired-instance' },
    sync: { async startCollection(name) {
      assert.ok(['business_commands', 'desktop_files'].includes(name));
      return name === 'desktop_files' ? { state: peer } : {};
    } },
    commandBus: { async dispatch(command) {
      assert.equal(command.command_type, 'ctox.workjet.jour_fixe.meeting.read'); commands.push(command);
      return { command_id: command.id, target_record_id: command.record_id, payload: command.payload,
        status: 'completed', ok: true, result: { ok: true, meeting: structuredClone(complete) } };
    } },
  };
  const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
  const start = source.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS');
  const end = source.indexOf('async function waitForSyncBridgeReady', start);
  const context = { state, crypto: webcrypto, AbortController, TextEncoder, setTimeout, clearTimeout,
    actorContext: session => ({ id: session.id }), readJourFixeNarration, validateNarrationRead, validateJourFixeValue, JOUR_FIXE_SCHEMA };
  vm.runInNewContext(source.slice(start, end) + '\nglobalThis.invoke = workjetProjectControl;', context);
  const result = await context.invoke(f.request);
  assert.equal(result.rangeSha256, await sha256Hex(f.bytes));
  assert.equal(commands.length, 2);
  assert.equal(new Set(commands.map(command => command.id)).size, 2);
  assert.equal(new Set(revisions).size, 2, 'fresh native tokens before and after demand bytes');
  const previous = peer.demandFileLoader.fetchFile;
  peer.demandFileLoader.fetchFile = async (...args) => { const value = await previous(...args); generation = 'replaced-peer'; return value; };
  await assert.rejects(context.invoke(f.request), { code: 'NARRATION_SCOPE_CHANGED' });
});
