import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, load, makeCommand } from './_ctx.mjs';
import { extractJsonObject, truncateTextPreserve, CV_PRINT_PDF_MAX_BYTES } from '../ctox.cv_print.apply_parse.js';

const T = 'ctox.cv_print.apply_parse';
const PAYLOAD = {
  title: 'CV strukturieren: ada.pdf', chat_id: 'chat_1', source_file_id: 'file_1', generation_id: 'gen_1',
  filename: 'ada.pdf', mime_type: 'application/pdf', size_bytes: 2048, sha256: 'abc', document_id: 'doc_1', version_id: 'doc_1_v1',
  writeback_contract: { command_type: T, target_collection: 'document_versions', document_id: 'doc_1', expected_model_schema: 'ctox.cv_print_profile.v1' },
};
// Ältere Profile: nach dem Kern-Import desktop_files→files liegt die Desktop-Datei unter derselben id in files.
const LEGACY_REF = 'c'.repeat(64);
const seed = () => ({
  files: [{ id: 'file_1', size: 2048, ref: LEGACY_REF, name: 'ada.pdf', mime: 'application/pdf' }],
  documents: [{ id: 'doc_1', document_id: 'doc_1', title: 'ada.pdf', owner: 'ui', display_cache: { thumb: 'x' } }],
  document_versions: [{ id: 'doc_1_v1', document_id: 'doc_1', version: 1 }, { id: 'other_v7', document_id: 'other', version: 7 }],
});
const caps = { 'doc.extract': { text: 'Ada Lovelace\n```code```' }, 'agent.task.enqueue': { task_id: 'task_9' } };

test('default: PDF-Text über cap.doc.extract, Prompt wie cv_print_command_prompt, Queue mit Rückkehr-Hooks', async () => {
  const ctx = createCtx({ seed: seed(), caps });
  const out = await run(ctx, T, PAYLOAD, { id: 'cmd_1' });
  assert.deepEqual(out, { task_id: 'task_9', status: 'queued' });
  assert.deepEqual(ctx.$.capsTo('doc.extract')[0].args[0], { blob_id: { ref: LEGACY_REF }, kind: 'pdf' });
  const task = ctx.$.capsTo('agent.task.enqueue')[0].args[0];
  assert.equal(task.skill, 'cv-print-builder/parser');
  assert.equal(task.on_complete, 'ctox.cv_print.apply_parse.done');
  assert.equal(task.on_failed, 'ctox.cv_print.apply_parse.failed');
  assert.equal(task.idempotency_key, 'cmd_1');
  assert.equal(task.payload.command.id, 'cmd_1');
  assert.deepEqual(Object.keys(task).sort(), ['idempotency_key', 'instruction', 'on_complete', 'on_failed', 'payload', 'skill', 'thread_key', 'title']);
  assert.ok(task.instruction.startsWith('Parse the extracted CV PDF text into a CTOX CV print profile JSON.\n\nCV PDF extracted text:'));
  assert.ok(task.instruction.includes("Ada Lovelace\n'''code'''"), 'Codezäune werden entschärft');
  assert.ok(task.instruction.includes(`- sha256: ${LEGACY_REF}`));
  assert.ok(task.instruction.endsWith('- document_id: doc_1\n- version_id: doc_1_v1\n- source_file_id: file_1\n\nReturn only the minified `ctox.cv_print_profile.v1` JSON. The CTOX service will persist it through `ctox.cv_print.apply_parse`.'));
  assert.equal(ctx.$.writes.length, 0);
});

test('default: fehlende Datei-ID, zu große PDF', async () => {
  await assert.rejects(run(createCtx({ seed: seed(), caps }), T, { document_id: 'doc_1' }), { code: 'invalid', message: 'cv-print parse command is missing source_file_id' });
  const big = seed();
  big.files[0].size = CV_PRINT_PDF_MAX_BYTES + 1;
  const ctx = createCtx({ seed: big, caps });
  await assert.rejects(run(ctx, T, PAYLOAD), { code: 'invalid', message: /too large/ });
  assert.equal(ctx.$.capsTo('doc.extract').length, 0);
});

test('onReply: neue Version max+1 je Dokument, Umschlag normalisiert, Dokument gepatcht', async () => {
  const ctx = createCtx({ seed: seed() });
  const { onReply } = await load(T);
  const reply = { task_id: 't', text: 'Hier: ```json\n{"candidate":{"name":" Ada ","currentRole":"CTO","email":"ada@x.io"},"print":{"template":"bold"},"workflow":{"diagnostics":["d"]}}\n```' };
  const out = await onReply(makeCommand(T, PAYLOAD, { id: 'cmd_1' }), reply, ctx);
  assert.deepEqual(out, { document_id: 'doc_1', version_id: 'doc_1_v2', version: 2, candidate_name: 'Ada' });
  const version = ctx.$.doc('document_versions', 'doc_1_v2');
  assert.equal(version.source_kind, 'cv_pdf_parse');
  assert.equal(version.blob_id, 'file_1');
  assert.deepEqual(version.diagnostics, ['d']);
  assert.equal(version.model, null);
  assert.equal(version.model_json.schema, 'ctox.cv_print_profile.v1');
  assert.deepEqual(version.model_json.workflow, { diagnostics: ['d'], phase: 'review', view_mode: 'split', command_id: 'cmd_1' });
  assert.deepEqual(version.model_json.source, { desktop_file_id: 'file_1', generation_id: 'gen_1', filename: 'ada.pdf', mime_type: 'application/pdf', size_bytes: 2048, sha256: 'abc' });
  assert.deepEqual(version.model_json.print, { template: 'bold', anonymize: false, showLogo: true, logoDataUrl: '', preset: 'standard', overrides: {} });
  const doc = ctx.$.doc('documents', 'doc_1');
  assert.equal(doc.owner, 'ui', 'vorhandene Felder bleiben');
  assert.equal(doc.current_version_id, 'doc_1_v2');
  assert.equal(doc.status, 'review');
  assert.equal(doc.title, 'Ada');
  assert.equal(doc.index_text, 'Ada · CTO · ada@x.io');
  assert.deepEqual(doc.display_cache, { thumb: 'x', phase: 'review', candidate_name: 'Ada' });
});

test('onReply: leere Antwort, kein JSON, ohne Namen → Unbenanntes Profil', async () => {
  const { onReply } = await load(T);
  const ctx = createCtx({ seed: seed() });
  await assert.rejects(onReply(makeCommand(T, PAYLOAD), { text: '  ' }, ctx), { code: 'invalid', message: 'cv-print parse reply was empty' });
  await assert.rejects(onReply(makeCommand(T, PAYLOAD), { text: 'kein json' }, ctx), { code: 'invalid', message: 'cv-print parse reply did not contain a JSON profile object' });
  const out = await onReply(makeCommand(T, { ...PAYLOAD, document_id: 'doc_new' }), { text: '{}' }, ctx);
  assert.equal(out.candidate_name, 'Unbenanntes Profil');
  assert.equal(out.version, 1);
  assert.equal(ctx.$.doc('documents', 'doc_new').document_id, 'doc_new');
});

test('Hilfen: extractJsonObject mit Strings/Klammern, truncateTextPreserve', () => {
  assert.deepEqual(extractJsonObject('x {"a":"}{","b":{"c":1}} y'), { a: '}{', b: { c: 1 } });
  assert.equal(extractJsonObject('[1,2]'), undefined);
  const long = 'a'.repeat(100);
  const cut = truncateTextPreserve(long, 90);
  assert.ok(cut.startsWith('a'.repeat(10) + '\n... truncated'));
  assert.equal(truncateTextPreserve('kurz', 90), 'kurz');
});

// Strang CVH: neue Profile tragen die Datei-Ref aus api.files (Plattform-Collection `files`, id/ref/size).
const REF = 'a'.repeat(64);
const REF_PAYLOAD = {
  title: 'CV strukturieren: bob.pdf', chat_id: 'chat_cv_doc_2', filename: 'bob.pdf', mime_type: 'application/pdf',
  size_bytes: 4096, sha256: REF, document_id: 'doc_2', version_id: 'doc_2_v1',
  source_file_ref: REF,
  source_file: { kind: 'file', ref: REF, file_id: 'cv_file_doc_2', name: 'bob.pdf', mime_type: 'application/pdf' },
};
const refSeed = (size = 4096) => ({
  files: [{ id: 'cv_file_doc_2', name: 'bob.pdf', mime: 'application/pdf', size, ref: REF, owner_app: 'cv-print-builder' }],
  documents: [{ id: 'doc_2', document_id: 'doc_2', title: 'bob.pdf' }],
  document_versions: [{ id: 'doc_2_v1', document_id: 'doc_2', version: 1 }],
});
const refCaps = { 'doc.extract': { text: 'Bob Builder', name: 'bob.pdf', mime_type: 'application/pdf', size_bytes: 4096, content_hash: REF }, 'agent.task.enqueue': { task_id: 'task_ref' } };

test('neues Profil mit source_file_ref: doc.extract({kind:file, ref}), Größe aus files, Ref bleibt in der Version', async () => {
  const ctx = createCtx({ seed: refSeed(), caps: refCaps });
  const out = await run(ctx, T, REF_PAYLOAD, { id: 'cmd_ref' });
  assert.deepEqual(out, { task_id: 'task_ref', status: 'queued' });
  const extract = ctx.$.capsTo('doc.extract');
  assert.equal(extract.length, 1);
  assert.deepEqual(extract[0].args[0], { blob_id: { ref: REF }, kind: 'pdf' });
  const task = ctx.$.capsTo('agent.task.enqueue')[0].args[0];
  assert.ok(task.instruction.includes('CV PDF extracted text:\nThe CTOX daemon read this PDF from Business OS files'));
  assert.ok(task.instruction.includes(`- source_file_id: cv_file_doc_2\n- source_file_ref: ${REF}\n- filename: bob.pdf`));
  assert.ok(task.instruction.includes('Bob Builder'));
  assert.ok(!task.instruction.includes('desktop_files'), 'kein desktop_files-Bezug bei neuen Profilen');
  assert.equal(ctx.$.writes.length, 0);

  const { onReply } = await load(T);
  const reply = await onReply(makeCommand(T, REF_PAYLOAD, { id: 'cmd_ref' }), { text: '{"candidate":{"name":"Bob"}}' }, ctx);
  assert.deepEqual(reply, { document_id: 'doc_2', version_id: 'doc_2_v2', version: 2, candidate_name: 'Bob' });
  const version = ctx.$.doc('document_versions', 'doc_2_v2');
  assert.equal(version.blob_id, REF);
  assert.equal(version.model_json.source.file_ref, REF);
  assert.equal(version.model_json.source.file_id, 'cv_file_doc_2');
  assert.equal('desktop_file_id' in version.model_json.source, false);
  assert.deepEqual(ctx.$.writes.map((w) => w.collection).sort(), ['document_versions', 'documents']);
});

test('neues Profil: zu große Datei laut files.size, Lookup per Ref ohne file_id', async () => {
  const big = createCtx({ seed: refSeed(CV_PRINT_PDF_MAX_BYTES + 1), caps: refCaps });
  await assert.rejects(run(big, T, REF_PAYLOAD), { code: 'invalid', message: `cv-print source PDF ${REF} is too large (${CV_PRINT_PDF_MAX_BYTES + 1} bytes, max ${CV_PRINT_PDF_MAX_BYTES})` });
  assert.equal(big.$.capsTo('doc.extract').length, 0);

  const byRef = createCtx({ seed: refSeed(CV_PRINT_PDF_MAX_BYTES + 1), caps: refCaps });
  const onlyRef = { ...REF_PAYLOAD, source_file: undefined };
  await assert.rejects(run(byRef, T, onlyRef), { code: 'invalid', message: /too large/ });

  const kindOnly = createCtx({ seed: refSeed(), caps: refCaps });
  const { source_file_ref: _drop, ...viaSourceFile } = REF_PAYLOAD;
  await run(kindOnly, T, viaSourceFile);
  assert.deepEqual(kindOnly.$.capsTo('doc.extract')[0].args[0], { blob_id: { ref: REF }, kind: 'pdf' });
});

test('altes Profil ohne source_file_ref: Quelle per Kern-Import unter derselben id in files, Rückschrieb unverändert', async () => {
  const ctx = createCtx({ seed: seed(), caps });
  const out = await run(ctx, T, PAYLOAD, { id: 'cmd_old' });
  assert.deepEqual(out, { task_id: 'task_9', status: 'queued' });
  assert.deepEqual(ctx.$.capsTo('doc.extract')[0].args[0], { blob_id: { ref: LEGACY_REF }, kind: 'pdf' });
  const task = ctx.$.capsTo('agent.task.enqueue')[0].args[0];
  assert.ok(task.instruction.includes('- source_file_id: file_1\n- filename: ada.pdf'));
  assert.ok(!task.instruction.includes('source_file_ref'));
  assert.ok(!task.instruction.includes('desktop_files'));
  const { onReply } = await load(T);
  await onReply(makeCommand(T, PAYLOAD, { id: 'cmd_old' }), { text: '{"candidate":{"name":"Ada"}}' }, ctx);
  const version = ctx.$.doc('document_versions', 'doc_1_v2');
  assert.equal(version.blob_id, 'file_1');
  assert.equal(version.model_json.source.desktop_file_id, 'file_1');
  assert.equal('file_ref' in version.model_json.source, false);
});

test('Altprofil ohne Kern-Import: not_found statt desktop_files-Zugriff', async () => {
  const ctx = createCtx({ seed: { documents: [], document_versions: [] }, caps });
  await assert.rejects(run(ctx, T, PAYLOAD), { code: 'not_found', message: /Kern-Import/ });
  assert.equal(ctx.$.capsTo('doc.extract').length, 0);
});

test('Rückruf ctox.cv_print.apply_parse.done: Ausgangsbefehl aus agent.task.get, onReply schreibt die Version', async () => {
  const ctx = createCtx({
    seed: seed(),
    caps: { 'agent.task.get': { task_id: 'task_9', payload: { command: { id: 'cmd_1', type: T, payload: PAYLOAD } } } },
  });
  const out = await run(ctx, 'ctox.cv_print.apply_parse.done', { task_id: 'task_9', status: 'completed', result: { text: '{"candidate":{"name":"Ada"}}' } });
  assert.deepEqual(out, { document_id: 'doc_1', version_id: 'doc_1_v2', version: 2, candidate_name: 'Ada' });
  assert.deepEqual(ctx.$.capsTo('agent.task.get')[0].args[0], { task_id: 'task_9' });
  const failed = await run(ctx, 'ctox.cv_print.apply_parse.done', { task_id: 'task_9', status: 'failed', result: { error: 'boom' } });
  assert.deepEqual(failed, { ok: false, status: 'failed', error: 'boom', applied: false }, 'Version wartet nicht auf diesen Lauf → keine Wirkung');
});
