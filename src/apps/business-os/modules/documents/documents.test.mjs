import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

async function importBrowserBundle(relativePath) {
  const bundledModule = await build({
    entryPoints: [fileURLToPath(new URL(relativePath, import.meta.url))],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    write: false,
  });

  const [{ text: bundledSource }] = bundledModule.outputFiles;
  return import(`data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`);
}

const { __documentsTestHooks: hooks } = await importBrowserBundle('./index.js');

test('document records without is_deleted are active', () => {
  assert.equal(hooks.isActiveDocumentRecord({ id: 'doc_1' }), true);
  assert.equal(hooks.isActiveDocumentRecord({ id: 'doc_1', is_deleted: false }), true);
  assert.equal(hooks.isActiveDocumentRecord({ id: 'doc_1', is_deleted: true }), false);
});

test('only superseded draft blobs are reclaimed, never the original or current blob', () => {
  // Successive autosaves: the previous draft blob is collectable.
  assert.equal(hooks.isReclaimableDraftBlob('v1_draft_100', 'v1_draft_200'), true);
  // The original imported source blob must be preserved on first edit.
  assert.equal(hooks.isReclaimableDraftBlob('v1_blob', 'v1_draft_200'), false);
  // Never delete the blob the version still points at.
  assert.equal(hooks.isReclaimableDraftBlob('v1_draft_200', 'v1_draft_200'), false);
  // No previous blob -> nothing to reclaim.
  assert.equal(hooks.isReclaimableDraftBlob('', 'v1_draft_200'), false);
});

test('visibleDocuments filters active normalized rows by status, tag, search, and sort', () => {
  const state = {
    searchQuery: 'vertrag',
    statusFilter: 'Draft',
    tagFilter: 'kunde-a',
    sortBy: 'title_asc',
    documents: [
      hooks.normalizeDocumentRecord({
        id: 'doc_2',
        title: 'Zeta Vertrag',
        filename: 'zeta.md',
        status: 'Draft',
        tags: ['kunde-a'],
        updated_at_ms: 20,
      }),
      hooks.normalizeDocumentRecord({
        id: 'doc_1',
        title: 'Alpha Vertrag',
        filename: 'alpha.docx',
        status: 'Draft',
        tags: ['kunde-a'],
        updated_at_ms: 10,
      }),
      hooks.normalizeDocumentRecord({
        id: 'doc_3',
        title: 'Alpha Angebot',
        filename: 'angebot.docx',
        status: 'Final',
        tags: ['kunde-a'],
        updated_at_ms: 30,
      }),
    ],
  };

  assert.deepEqual(hooks.visibleDocuments(state).map((record) => record.id), ['doc_1', 'doc_2']);
});

test('new document validation requires title, runbook, and prompt', () => {
  assert.equal(hooks.validateNewDocumentInput({ title: 'Report', runbookId: 'research.report.auto', prompt: '' }).valid, false);
  assert.equal(hooks.validateNewDocumentInput({ title: 'Report', runbookId: '', prompt: 'Analyse' }).valid, false);
  assert.equal(hooks.validateNewDocumentInput({ title: 'Report', runbookId: 'research.report.auto', prompt: 'Analyse' }).valid, true);
});

test('import validation requires a supported file', () => {
  assert.equal(hooks.validateImportInput({ file: null }).valid, false);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'notes.md', { type: 'text/plain' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'notes.txt', { type: 'text/plain' }) }).valid, false);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'image.png', { type: 'image/png' }) }).valid, false);
});
