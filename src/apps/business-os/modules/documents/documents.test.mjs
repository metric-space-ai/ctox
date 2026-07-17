import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';

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

test('documents module declares the shared Knowledge collections', async () => {
  const moduleJson = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));
  const registryJson = JSON.parse(await readFile(new URL('../registry.json', import.meta.url), 'utf8'));
  const collectionSchema = JSON.parse(await readFile(new URL('./collections.schema.json', import.meta.url), 'utf8'));
  const registryModule = registryJson.modules.find((item) => item.id === 'documents');
  const required = ['knowledge_items', 'knowledge_runbooks', 'knowledge_tables'];
  for (const name of required) {
    assert.ok(moduleJson.collections.includes(name));
    assert.ok(registryModule.collections.includes(name));
    assert.ok(collectionSchema.collections[name]);
  }
});

test('document knowledge context references a chunked table only once', () => {
  const tables = hooks.mergeKnowledgeTableReferences([
    {
      id: 'table:loads:chunk:0001',
      payload: {
        logical_table_id: 'table:loads',
        domain: 'drone_bearing_design_verified',
        chunk_index: 1,
        chunk_count: 2,
      },
    },
    {
      id: 'table:loads',
      payload: {
        logical_table_id: 'table:loads',
        domain: 'drone_bearing_design_verified',
        chunk_index: 0,
        chunk_count: 2,
      },
    },
  ]);

  assert.equal(tables.length, 1);
  assert.equal(tables[0].id, 'table:loads');
  assert.equal(tables[0].domain, 'drone_bearing_design_verified');
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

test('typed runtime settings select CTOX Documents by default and preserve explicit legacy rollback', () => {
  assert.equal(hooks.officeEngineFromSettings({ office: { documents_engine: 'ctox_office' } }), 'ctox_documents');
  assert.equal(hooks.officeEngineFromSettings({ office: { documents_engine: 'ctox_documents' } }), 'ctox_documents');
  assert.equal(hooks.officeEngineFromSettings({ office: { documents_engine: 'legacy' } }), 'legacy');
  assert.equal(hooks.officeEngineFromSettings({}), 'ctox_documents');
});

test('CTOX Documents permissions expose comments and review only with full write access', () => {
  const writable = hooks.ctoxDocumentsPermissions({
    permissions: { canWriteCollection: () => true },
  });
  assert.deepEqual(writable, { read: true, write: true, export: true, comment: true, review: true });

  const readOnly = hooks.ctoxDocumentsPermissions({
    permissions: { canWriteCollection: (name) => name !== 'document_versions' },
  });
  assert.deepEqual(readOnly, { read: true, write: false, export: true, comment: false, review: false });
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

test('knowledge selection supports explicit skills and automatic topic matching', () => {
  const state = {
    knowledgeItems: [
      hooks.normalizeKnowledgeRecord({ id: 'skill:bearings', kind: 'skill', title: 'Drone Bearing Loads', summary: 'Propeller torque and bearing force', payload: { domain: 'drone_bearing_design' }, updated_at_ms: 20 }),
      hooks.normalizeKnowledgeRecord({ id: 'skill:markets', kind: 'skill', title: 'Market Research', summary: 'Vendors and pricing', payload: { domain: 'market' }, updated_at_ms: 30 }),
    ],
    knowledgeRunbooks: [{ id: 'runbook:bearing-report', kind: 'runbook', payload: { domain: 'drone_bearing_design' } }],
    knowledgeTables: [{ id: 'table:bearing-loads', kind: 'dataframe', payload: { domain: 'drone_bearing_design' } }],
  };

  const automatic = hooks.resolveKnowledgeContext(state, 'auto', 'Analyse propeller torque for drone bearings');
  assert.equal(automatic.id, 'skill:bearings');
  assert.equal(automatic.selection_mode, 'auto');
  assert.deepEqual(automatic.table_ids, ['table:bearing-loads']);
  assert.deepEqual(automatic.linked_runbook_ids, ['runbook:bearing-report']);

  const manual = hooks.resolveKnowledgeContext(state, 'skill:markets', 'bearing loads');
  assert.equal(manual.id, 'skill:markets');
  assert.equal(manual.selection_mode, 'manual');
});

test('documents become stale when their linked knowledge item is newer', () => {
  const record = { linked_records: [{ type: 'knowledge', id: 'skill:bearings', title: 'Bearing Loads', updated_at_ms: 100 }] };
  const state = { knowledgeItems: [{ id: 'skill:bearings', updated_at_ms: 101 }] };
  assert.equal(hooks.documentKnowledgeLink(record).id, 'skill:bearings');
  assert.equal(hooks.isDocumentKnowledgeStale(state, record), true);
  state.knowledgeItems[0].updated_at_ms = 100;
  assert.equal(hooks.isDocumentKnowledgeStale(state, record), false);
});

test('import validation requires a supported file', () => {
  assert.equal(hooks.validateImportInput({ file: null }).valid, false);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'notes.md', { type: 'text/plain' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'notes.txt', { type: 'text/plain' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'image.png', { type: 'image/png' }) }).valid, false);
});

test('file-open deduplication reuses the imported document with the same source hash', () => {
  const records = [
    { id: 'doc_other', source_sha256: 'aaaa' },
    { id: 'doc_report', source_sha256: 'CAFE' },
  ];
  assert.equal(hooks.documentBySourceSha(records, 'cafe')?.id, 'doc_report');
  assert.equal(hooks.documentBySourceSha(records, 'missing'), null);
});

test('document blob chunks are persisted with one bulk write', async () => {
  const bulkWrites = [];
  const blobChunks = {
    bulkUpsert: async (docs) => { bulkWrites.push(docs); },
    insert: async () => { throw new Error('document_blob_chunks insert must not run per chunk'); },
  };
  const ctx = {
    db: {
      collection(name) {
        if (name === 'document_blob_chunks') return blobChunks;
        if (name === 'documents' || name === 'document_versions') return {};
        return null;
      },
    },
  };

  const bytes = new Uint8Array(260 * 1024);
  bytes.fill(66);
  await hooks.saveBlobChunks(ctx, {
    blobId: 'blob_bulk',
    documentId: 'doc_bulk',
    versionId: 'version_bulk',
    mimeType: 'application/octet-stream',
    bytes,
  });

  assert.equal(bulkWrites.length, 1, 'blob chunks are written through one bulkUpsert call');
  assert.ok(bulkWrites[0].length > 1, 'test payload spans multiple chunk documents');
});
