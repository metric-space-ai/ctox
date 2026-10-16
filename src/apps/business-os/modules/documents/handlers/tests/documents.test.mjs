// Origin: CTOX
// License: Apache-2.0
// node --test suite for the documents v4 handlers. Capabilities are simulated
// through the cap proxy in _ctx.mjs; a missing cap throws {code:'unavailable'}.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';

import prepare from '../office.document.prepare.js';
import commit from '../office.document.commit.js';
import freeze from '../office.document.freeze_email_content.js';
import exportDoc from '../office.document.export.js';
import settingsSave from '../ctox.office.settings.save.js';
import { shortHash } from '../_lib/office.js';

const NOW = 1_760_000_000_000;

function baseData() {
  return {
    documents: {
      doc_1: {
        id: 'doc_1', title: 'Angebot', filename: 'angebot.docx',
        mime_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
        status: 'Imported', current_version_id: 'doc_1_v1', created_at_ms: 1,
      },
    },
    document_versions: {
      doc_1_v1: {
        id: 'doc_1_v1', document_id: 'doc_1', version: 1, blob_id: 'doc_1_v1_src',
        editor_blob_id: 'doc_1_v1_editor', source_sha256: 'base-src', editor_sha256: 'base-editor',
        created_at_ms: 1, updated_at_ms: 1,
      },
    },
  };
}

// Kernform §16.32: BlobRef {ref}; session {kind, source:{ref}, editor:{ref}}.
const preparedCap = async (args) => ({
  session: { kind: args.kind, source: { ref: args.blob_id.ref }, editor: { ref: 'editor-sha' } },
  model: { ok: true }, manifest: { primary_text: 'Hallo', parts: [1, 2] },
  protocol: 'euro-office-document', protocol_version: 3, sha256: 'editor-sha',
});
const FILES = {
  files: {
    file_editor: { id: 'file_editor', ref: 'editor-sha', size: 77 },
    file_canon: { id: 'file_canon', ref: 'canon-sha', size: 99 },
    file_new_canon: { id: 'file_new_canon', ref: 'new-canonical-sha', size: 4096 },
    file_new_editor: { id: 'file_new_editor', ref: 'new-editor-sha', size: 300 },
    staged_editor: { id: 'staged_editor', ref: 'staged-sha', size: 50 },
    file_pkg: { id: 'file_pkg', ref: 'pkg-sha', size: 2048 },
    file_html: { id: 'file_html', ref: 'html-sha', size: 10 },
    file_text: { id: 'file_text', ref: 'text-sha', size: 4 },
  },
};
const withFiles = (data) => ({ ...data, ...FILES });

test('office.document.prepare stamps editor metadata on the version and returns the prepare result', async () => {
  const env = createTestCtx({ data: withFiles(baseData()), now: NOW, caps: { 'office.edit.prepare': preparedCap } });
  const result = await prepare(cmd('office.document.prepare', { document_id: 'doc_1' }), env.ctx);
  assert.equal(result.ok, true);
  assert.equal(result.operation, 'prepare');
  assert.equal(result.editor_blob_id, 'file_editor', 'editor ref is mapped to its files id');
  assert.equal(result.blob_id, 'doc_1_v1_src');
  const version = env.doc('document_versions', 'doc_1_v1');
  assert.equal(version.conversion_state, 'prepared');
  assert.equal(version.editor_blob_id, 'file_editor');
  assert.equal(version.editor_sha256, 'editor-sha');
  assert.equal(version.editor_protocol_version, 3);
  assert.equal(version.updated_at_ms, NOW);
  const [capCall] = env.calls.caps;
  assert.equal(capCall.name, 'office.edit.prepare');
  assert.deepEqual(Object.keys(capCall.args[0]).sort(), ['blob_id', 'implemented_features', 'kind']);
  assert.deepEqual(capCall.args[0].blob_id, { ref: 'base-src' }, 'BlobRef {ref} from the version source sha');
  assert.equal(capCall.args[0].implemented_features.length, 11);
});

test('office.document.prepare carries the delimited canonical rewrite when the capability converts', async () => {
  const env = createTestCtx({
    data: withFiles(baseData()), now: NOW,
    caps: { 'office.edit.prepare': async (a) => {
      const p = await preparedCap(a);
      return { ...p, session: { ...p.session, source: { ref: 'canon-sha' } } };
    } },
  });
  await prepare(cmd('office.document.prepare', { document_id: 'doc_1' }), env.ctx);
  const version = env.doc('document_versions', 'doc_1_v1');
  assert.equal(version.blob_id, 'file_canon');
  assert.equal(version.source_sha256, 'canon-sha');
  assert.equal(version.original_blob_id, 'doc_1_v1_src');
  assert.equal(version.canonical_source_kind, 'delimited_text_to_xlsx');
});

test('office.document.prepare rejects a version without a canonical blob', async () => {
  const data = baseData();
  delete data.document_versions.doc_1_v1.blob_id;
  delete data.document_versions.doc_1_v1.source_sha256;
  const env = createTestCtx({ data, now: NOW, caps: { 'office.edit.prepare': preparedCap } });
  const err = await rejects(prepare(cmd('office.document.prepare', { document_id: 'doc_1' }), env.ctx));
  assert.equal(err.code, 'invalid');
});

const applyCap = async (args) => ({
  session: { kind: args.session.kind, source: { ref: 'new-canonical-sha' }, editor: { ref: 'new-editor-sha' } },
  blob_id: { ref: 'new-canonical-sha' }, sha256: 'new-canonical-sha',
  protocol: 'euro-office-document', protocol_version: 3, model: {},
  manifest: { primary_text: 'Neuer Text', parts: [1] },
});

test('office.document.commit creates a new version, bumps the document and returns commit metadata', async () => {
  const env = createTestCtx({ data: withFiles(baseData()), now: NOW, caps: { 'office.edit.apply': applyCap } });
  const command = cmd('office.document.commit', {
    document_id: 'doc_1', base_version_id: 'doc_1_v1', editor_blob_id: 'staged_editor',
  }, { id: 'cmd_commit_1' });
  const result = await commit(command, env.ctx);
  assert.equal(result.ok, true);
  assert.equal(result.operation, 'commit');
  assert.match(result.version_id, /^doc_1_office_v2_/);
  assert.equal(result.source_sha256, 'new-canonical-sha');
  assert.equal(result.blob_id, 'file_new_canon');
  assert.equal(result.editor_blob_id, 'file_new_editor');
  assert.equal(result.bytes, 4096);
  const [call] = env.calls.caps;
  assert.equal(call.name, 'office.edit.apply');
  assert.deepEqual(call.args[0], {
    session: { kind: 'document', source: { ref: 'base-src' }, editor: { ref: 'base-editor' } },
    changes: { ref: 'staged-sha' }, base_version: 'base-editor', implemented_features: [],
  });
  const doc = env.doc('documents', 'doc_1');
  assert.equal(doc.current_version_id, result.version_id);
  assert.equal(doc.status, 'Draft');
  assert.equal(doc.index_text, 'Neuer Text');
  const version = env.doc('document_versions', result.version_id);
  assert.equal(version.version, 2);
  assert.equal(version.base_version_id, 'doc_1_v1');
  assert.equal(version.source_kind, 'office_edited_docx');
  assert.equal(version.model_json.parts, 1);
  assert.equal(version.staged_editor_blob_id, 'staged_editor');
});

test('office.document.commit rejects a staged editor blob whose ref differs from editor_sha256', async () => {
  const env = createTestCtx({ data: withFiles(baseData()), now: NOW, caps: { 'office.edit.apply': applyCap } });
  const err = await rejects(commit(cmd('office.document.commit', {
    document_id: 'doc_1', base_version_id: 'doc_1_v1', editor_blob_id: 'staged_editor', editor_sha256: 'other-sha',
  }, { id: 'cmd_commit_x' }), env.ctx));
  assert.equal(err.code, 'conflict');
  assert.equal(env.calls.caps.length, 0);
});

test('office.document.commit prepares a session first when the base version was never prepared', async () => {
  const data = withFiles(baseData());
  delete data.document_versions.doc_1_v1.editor_blob_id;
  delete data.document_versions.doc_1_v1.editor_sha256;
  const env = createTestCtx({ data, now: NOW, caps: { 'office.edit.prepare': preparedCap, 'office.edit.apply': applyCap } });
  await commit(cmd('office.document.commit', {
    document_id: 'doc_1', base_version_id: 'doc_1_v1', editor_blob_id: 'staged_editor',
  }, { id: 'cmd_commit_p' }), env.ctx);
  assert.deepEqual(env.calls.caps.map((c) => c.name), ['office.edit.prepare', 'office.edit.apply']);
  assert.equal(env.calls.caps[1].args[0].base_version, 'editor-sha');
});

test('office.document.commit replays idempotently when the target version was already written (crash recovery)', async () => {
  // Faithful to Rust: the replay guard covers a retry where the version row
  // was committed but the document row was not yet advanced. current_version_id
  // therefore still points at the base version.
  const targetVersionId = `doc_1_office_v2_${shortHash('cmd_commit_1')}`;
  const data = baseData();
  data.document_versions[targetVersionId] = {
    id: targetVersionId, document_id: 'doc_1', version: 2, blob_id: `${targetVersionId}_blob`,
    editor_blob_id: `${targetVersionId}_blob`, source_sha256: 'persisted-sha', editor_sha256: 'persisted-editor',
    office_manifest: { primary_text: 'x' }, diagnostics: [],
  };
  const env = createTestCtx({ data, now: NOW, caps: { 'office.edit.apply': applyCap } });
  const command = cmd('office.document.commit', {
    document_id: 'doc_1', base_version_id: 'doc_1_v1', editor_blob_id: 'staged_editor',
  }, { id: 'cmd_commit_1' });
  const replay = await commit(command, env.ctx);
  assert.equal(replay.idempotent_replay, true);
  assert.equal(replay.version_id, targetVersionId);
  assert.equal(replay.source_sha256, 'persisted-sha');
  assert.equal(env.calls.caps.length, 0, 'replay must not call the office engine again');
});

test('office.document.commit rejects a stale base version with conflict', async () => {
  const data = baseData();
  // An older version exists but the document has already advanced past it.
  data.document_versions.doc_1_v0 = { ...data.document_versions.doc_1_v1, id: 'doc_1_v0', version: 0 };
  const env = createTestCtx({ data, now: NOW, caps: { 'office.edit.apply': applyCap } });
  const err = await rejects(commit(cmd('office.document.commit', {
    document_id: 'doc_1', base_version_id: 'doc_1_v0', editor_blob_id: 'staged_editor',
  }, { id: 'cmd_commit_2' }), env.ctx));
  assert.equal(err.code, 'conflict');
});

test('office.document.commit requires base_version_id and editor_blob_id', async () => {
  const env = createTestCtx({ data: baseData(), now: NOW, caps: { 'office.edit.apply': applyCap } });
  const e1 = await rejects(commit(cmd('office.document.commit', { document_id: 'doc_1', editor_blob_id: 'x' }, { id: 'c' }), env.ctx));
  assert.equal(e1.code, 'invalid');
  const e2 = await rejects(commit(cmd('office.document.commit', { document_id: 'doc_1', base_version_id: 'doc_1_v1' }, { id: 'c' }), env.ctx));
  assert.equal(e2.code, 'invalid');
});

test('office.document.freeze_email_content prepares a session, renders email_html and stamps the frozen artifact', async () => {
  const data = withFiles(baseData());
  data.document_versions.doc_1_v1.source_sha256 = 'base-src';
  const env = createTestCtx({
    data, now: NOW,
    caps: {
      'office.edit.prepare': preparedCap,
      'office.render': async () => ({
        blob_id: { ref: 'html-sha' }, sha256: 'html-sha', text_blob: { ref: 'text-sha' },
        assets: [{ content_id: 'img1', mime_type: 'image/png', blob_id: { ref: 'pkg-sha' } }], compiler_id: 'email-v1',
      }),
    },
  });
  const result = await freeze(cmd('office.document.freeze_email_content', { document_id: 'doc_1' }), env.ctx);
  assert.equal(result.ok, true);
  assert.equal(result.artifact.state, 'frozen');
  assert.deepEqual(env.calls.caps.map((c) => c.name), ['office.edit.prepare', 'office.render']);
  assert.deepEqual(env.calls.caps[1].args[0], {
    session: { kind: 'document', source: { ref: 'base-src' }, editor: { ref: 'editor-sha' } }, to: 'email_html',
  });
  const version = env.doc('document_versions', 'doc_1_v1');
  const artifact = version.email_render_artifact;
  assert.equal(artifact.html_blob_id, 'file_html');
  assert.equal(artifact.html_sha256, 'html-sha');
  assert.equal(artifact.text_blob_id, 'file_text');
  assert.equal(artifact.text_bytes, 4);
  assert.deepEqual(artifact.assets[0], { content_id: 'img1', mime_type: 'image/png', blob_id: 'file_pkg', sha256: 'pkg-sha', bytes: 2048 });
  assert.equal(artifact.frozen_at_ms, NOW);
});

test('office.document.export prepares, then renders docx from the session', async () => {
  const env = createTestCtx({
    data: withFiles(baseData()), now: NOW,
    caps: {
      'office.edit.prepare': preparedCap,
      'office.render': async () => ({ blob_id: { ref: 'pkg-sha' }, sha256: 'pkg-sha', mime_type: 'application/x' }),
    },
  });
  const result = await exportDoc(cmd('office.document.export', { document_id: 'doc_1' }), env.ctx);
  assert.equal(result.ok, true);
  assert.equal(result.operation, 'export');
  assert.equal(result.blob_id, 'file_pkg');
  assert.equal(result.source_blob_id, 'doc_1_v1_src');
  assert.equal(result.source_sha256, 'pkg-sha');
  assert.equal(result.bytes, 2048);
  assert.equal(result.extension, 'docx');
  assert.deepEqual(env.calls.caps.map((c) => c.name), ['office.edit.prepare', 'office.render']);
  assert.equal(env.calls.caps[1].args[0].to, 'docx');
  assert.ok(env.calls.caps[1].args[0].session.editor.ref);
});

test('office.document.export as pdf is refused as unavailable before any core call', async () => {
  const env = createTestCtx({ data: withFiles(baseData()), now: NOW, caps: { 'office.edit.prepare': preparedCap } });
  const err = await rejects(exportDoc(cmd('office.document.export', { document_id: 'doc_1', format: 'pdf' }), env.ctx));
  assert.equal(err.code, 'unavailable');
  assert.equal(env.calls.caps.length, 0);
});

test('ctox.office.settings.save forwards to the office settings capability', async () => {
  const env = createTestCtx({ data: {}, now: NOW, caps: { 'office.settings.save': async (s) => ({ saved: s }) } });
  const result = await settingsSave(cmd('ctox.office.settings.save', { locale: 'de' }), env.ctx);
  assert.equal(result.ok, true);
  assert.deepEqual(result.office.saved, { locale: 'de' });
});

test('a missing capability surfaces as an unavailable error', async () => {
  const env = createTestCtx({ data: baseData(), now: NOW });
  const err = await rejects(prepare(cmd('office.document.prepare', { document_id: 'doc_1' }), env.ctx));
  assert.equal(err.code, 'unavailable');
});

// research.systematic.report.create/.complete/.failed und document.*: automation.test.mjs (Strang SKL-B3).
