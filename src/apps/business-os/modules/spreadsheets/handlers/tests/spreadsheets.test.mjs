// Origin: CTOX
// License: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { shortHash } from '../_lib/office.js';

import prepare from '../office.spreadsheet.prepare.js';
import commit from '../office.spreadsheet.commit.js';
import exportSheet from '../office.spreadsheet.export.js';

const NOW = 1_760_000_000_000;

function baseData() {
  return {
    spreadsheets: {
      sh_1: {
        id: 'sh_1', title: 'Budget', filename: 'budget.xlsx', mime_type: 'text/csv',
        status: 'Imported', current_version_id: 'sh_1_v1', created_at_ms: 1,
      },
    },
    spreadsheet_versions: {
      sh_1_v1: {
        id: 'sh_1_v1', spreadsheet_id: 'sh_1', version: 1, blob_id: 'sh_1_v1_src',
        editor_blob_id: 'sh_1_v1_editor', source_sha256: 'base-src', editor_sha256: 'base-editor',
        source_kind: 'csv', ingestion_kind: 'research_generated',
        knowledge_version: { version_id: 'knowledge-v7' },
        source_receipt_snapshot_hashes: ['sha256:ab'], created_at_ms: 1, updated_at_ms: 1,
      },
    },
  };
}

// Kernform §16.32: BlobRef {ref}; session {kind, source:{ref}, editor:{ref}}. Der CSV-Quellblob wird kanonisiert.
const preparedCap = async (args) => ({
  session: { kind: args.kind, source: { ref: 'canon-sha' }, editor: { ref: 'editor-sha' } },
  model: {}, manifest: { primary_text: 'A1', parts: [1] },
  protocol: 'euro-office-spreadsheet', protocol_version: 2, sha256: 'editor-sha',
});
const applyCap = async (args) => ({
  session: { kind: args.session.kind, source: { ref: 'new-sha' }, editor: { ref: 'new-editor' } },
  blob_id: { ref: 'new-sha' }, sha256: 'new-sha', protocol: 'euro-office-spreadsheet', protocol_version: 2,
  model: {}, manifest: { primary_text: 'Neu', parts: [1] },
});
const FILES = {
  files: {
    file_canon: { id: 'file_canon', ref: 'canon-sha', size: 9 },
    file_editor: { id: 'file_editor', ref: 'editor-sha', size: 9 },
    file_new: { id: 'file_new', ref: 'new-sha', size: 2048 },
    file_new_editor: { id: 'file_new_editor', ref: 'new-editor', size: 5 },
    staged: { id: 'staged', ref: 'staged-sha', size: 5 },
    file_pkg: { id: 'file_pkg', ref: 'pkg-sha', size: 4096 },
  },
};
const withFiles = (data) => ({ ...data, ...FILES });

test('office.spreadsheet.prepare asks for the single spreadsheet feature and writes the canonical rewrite', async () => {
  const env = createTestCtx({ data: withFiles(baseData()), now: NOW, caps: { 'office.edit.prepare': preparedCap } });
  const result = await prepare(cmd('office.spreadsheet.prepare', { spreadsheet_id: 'sh_1' }), env.ctx);
  assert.equal(result.ok, true);
  assert.equal(env.calls.caps[0].args[0].implemented_features.length, 1);
  const version = env.doc('spreadsheet_versions', 'sh_1_v1');
  assert.equal(env.calls.caps[0].name, 'office.edit.prepare');
  assert.deepEqual(env.calls.caps[0].args[0].blob_id, { ref: 'base-src' });
  assert.equal(version.blob_id, 'file_canon');
  assert.equal(version.source_sha256, 'canon-sha');
  assert.equal(version.editor_blob_id, 'file_editor');
  assert.equal(version.original_blob_id, 'sh_1_v1_src');
  assert.equal(version.conversion_state, 'prepared');
});

test('office.spreadsheet.commit creates v2 and carries ingestion/knowledge lineage forward', async () => {
  const env = createTestCtx({ data: withFiles(baseData()), now: NOW, caps: { 'office.edit.apply': applyCap } });
  const result = await commit(cmd('office.spreadsheet.commit', {
    spreadsheet_id: 'sh_1', base_version_id: 'sh_1_v1', editor_blob_id: 'staged',
  }, { id: 'cmd_sheet_commit' }), env.ctx);
  assert.equal(result.ok, true);
  assert.equal(result.editor_blob_id, 'file_new_editor');
  assert.equal(result.staged_editor_blob_id, 'staged');
  assert.equal(result.blob_id, 'file_new');
  assert.deepEqual(env.calls.caps[0].args[0].changes, { ref: 'staged-sha' });
  assert.deepEqual(env.calls.caps[0].args[0].session.editor, { ref: 'base-editor' });
  const version = env.doc('spreadsheet_versions', result.version_id);
  assert.equal(version.source_kind, 'office_edited_xlsx');
  assert.equal(version.model_json.type, 'xlsx');
  assert.equal(version.ingestion_kind, 'research_generated');
  assert.equal(version.knowledge_version.version_id, 'knowledge-v7');
  assert.deepEqual(version.source_receipt_snapshot_hashes, ['sha256:ab']);
  const sheet = env.doc('spreadsheets', 'sh_1');
  assert.equal(sheet.current_version_id, result.version_id);
});

test('office.spreadsheet.commit replays idempotently on a committed-version retry', async () => {
  const targetVersionId = `sh_1_office_v2_${shortHash('cmd_sheet_commit')}`;
  const data = baseData();
  data.spreadsheet_versions[targetVersionId] = {
    id: targetVersionId, spreadsheet_id: 'sh_1', version: 2, blob_id: `${targetVersionId}_blob`,
    editor_blob_id: `${targetVersionId}_blob`, source_sha256: 'persisted', editor_sha256: 'persisted-e',
    office_manifest: {}, diagnostics: [],
  };
  const env = createTestCtx({ data, now: NOW, caps: { 'office.edit.apply': applyCap } });
  const replay = await commit(cmd('office.spreadsheet.commit', {
    spreadsheet_id: 'sh_1', base_version_id: 'sh_1_v1', editor_blob_id: 'staged',
  }, { id: 'cmd_sheet_commit' }), env.ctx);
  assert.equal(replay.idempotent_replay, true);
  assert.equal(replay.version_id, targetVersionId);
  assert.equal(env.calls.caps.length, 0);
});

test('office.spreadsheet.commit rejects a stale base version', async () => {
  const data = baseData();
  data.spreadsheet_versions.sh_1_v0 = { ...data.spreadsheet_versions.sh_1_v1, id: 'sh_1_v0', version: 0 };
  const env = createTestCtx({ data, now: NOW, caps: { 'office.edit.apply': applyCap } });
  const err = await rejects(commit(cmd('office.spreadsheet.commit', {
    spreadsheet_id: 'sh_1', base_version_id: 'sh_1_v0', editor_blob_id: 'staged',
  }, { id: 'c' }), env.ctx));
  assert.equal(err.code, 'conflict');
});

test('office.spreadsheet.export prepares, then renders xlsx from the session', async () => {
  const env = createTestCtx({
    data: withFiles(baseData()), now: NOW,
    caps: {
      'office.edit.prepare': preparedCap,
      'office.render': async () => ({ blob_id: { ref: 'pkg-sha' }, sha256: 'pkg-sha', mime_type: 'application/x' }),
    },
  });
  const result = await exportSheet(cmd('office.spreadsheet.export', { spreadsheet_id: 'sh_1' }), env.ctx);
  assert.equal(result.extension, 'xlsx');
  assert.equal(result.blob_id, 'file_pkg');
  assert.equal(result.source_blob_id, 'sh_1_v1_src');
  assert.deepEqual(env.calls.caps.map((c) => c.name), ['office.edit.prepare', 'office.render']);
  assert.deepEqual(env.calls.caps[1].args[0], {
    session: { kind: 'spreadsheet', source: { ref: 'canon-sha' }, editor: { ref: 'editor-sha' } }, to: 'xlsx',
  });
});

test('office.spreadsheet.export as pdf is refused as unavailable before any core call', async () => {
  const env = createTestCtx({ data: withFiles(baseData()), now: NOW, caps: { 'office.edit.prepare': preparedCap } });
  const err = await rejects(exportSheet(cmd('office.spreadsheet.export', { spreadsheet_id: 'sh_1', format: 'pdf' }), env.ctx));
  assert.equal(err.code, 'unavailable');
  assert.equal(env.calls.caps.length, 0);
});

test('a module mismatch (document command sent to spreadsheets) is rejected as invalid', async () => {
  const env = createTestCtx({ data: withFiles(baseData()), now: NOW, caps: { 'office.edit.prepare': preparedCap } });
  const err = await rejects(prepare(cmd('office.document.prepare', { spreadsheet_id: 'sh_1' }), env.ctx));
  assert.equal(err.code, 'invalid');
});
