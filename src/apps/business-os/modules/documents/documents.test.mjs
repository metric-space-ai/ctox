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

test('document launch arguments resolve the requested record for initial and repeated app opens', () => {
  assert.equal(hooks.documentIdFromLaunchArgs({ record: 'doc_17' }), 'doc_17');
  assert.equal(hooks.documentIdFromLaunchArgs({ documentId: 'doc_18' }), 'doc_18');
  assert.equal(hooks.documentIdFromLaunchArgs({ record: '  ' }), '');
  assert.equal(hooks.versionIdFromLaunchArgs({ version: 'doc_17_v2' }), 'doc_17_v2');
  assert.equal(hooks.versionIdFromLaunchArgs({ versionId: 'doc_18_v3' }), 'doc_18_v3');
});

test('active editor teardown is awaited once across concurrent renders', async () => {
  let destroyCalls = 0;
  let releaseDestroy;
  const state = {
    editorHandle: {
      destroy: async () => {
        destroyCalls += 1;
        await new Promise((resolve) => { releaseDestroy = resolve; });
      },
    },
    editorDestroyPromise: null,
  };

  const first = hooks.destroyActiveEditor(state);
  const second = hooks.destroyActiveEditor(state);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(destroyCalls, 1);
  releaseDestroy();
  await Promise.all([first, second]);
  assert.equal(destroyCalls, 1);
  assert.equal(state.editorHandle, null);
  assert.equal(state.editorDestroyPromise, null);
});

test('only transient Office startup failures are retryable', () => {
  assert.equal(hooks.isTransientOfficeStartupError(new Error('CTOX product iframe load timed out')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('Office RPC timed out: editor.ready')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('CTOX Documents app-ready timed out')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('CTOX document fork SDK load timed out')), true);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('Document SHA-256 mismatch')), false);
  assert.equal(hooks.isTransientOfficeStartupError(new Error('Unsupported editor protocol')), false);
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

test('mail merge and series letter records remain one list entry while normal documents stay individual', () => {
  const grouped = hooks.groupDocumentRecords([
    hooks.normalizeDocumentRecord({
      id: 'merge_1',
      title: 'MURRELEKTRONIK 2022 - Welle 3',
      filename: 'murrel-serienbrief.docx',
      document_type: 'mail_merge',
      status: 'Created',
      mail_merge: { recipient_count: 19, requested_count: 19 },
      provenance: { app_id: 'crm', source: 'campaign-mail-merge' },
    }),
    hooks.normalizeDocumentRecord({
      id: 'letter_1',
      title: 'Einzelbrief',
      filename: 'einzelbrief.docx',
      document_type: 'word_document',
      status: 'Draft',
    }),
    hooks.normalizeDocumentRecord({
      id: 'note_1',
      title: 'Notiz',
      filename: 'notiz.md',
      document_type: 'markdown_document',
      status: 'Draft',
    }),
  ]);

  assert.equal(grouped.length, 3);
  const merge = grouped.find((entry) => entry.id === 'merge_1');
  assert.equal(merge.is_mail_merge, true);
  assert.equal(merge.document_type, 'mail_merge');
  assert.equal(merge.recipient_count, 19);
  assert.deepEqual(grouped.filter((entry) => !entry.is_mail_merge).map((entry) => entry.id).sort(), ['letter_1', 'note_1']);
});

test('legacy per-recipient campaign documents group by stable provenance and template identity', () => {
  const records = ['Bjarne Schäfer', 'Daniel Floris', 'Markus H. Niedermayer'].map((recipient, index) => (
    hooks.normalizeDocumentRecord({
      id: `legacy_${index + 1}`,
      title: `MURRELEKTRONIK 2022 - Welle 3 - GT - 12.10.2022 - ${recipient} - Brief Erstkontakt`,
      filename: `recipient-${index + 1}.docx`,
      document_type: 'word_document',
      status: 'Draft',
      current_version_id: `legacy_${index + 1}_v1`,
      template_ref: { template_id: 'brief-erstkontakt', version: 4 },
      provenance: {
        app_id: 'crm',
        source: 'campaign-mail-merge',
        selection_id: 4711,
        selectionmember_id: index + 100,
      },
    })
  ));
  const grouped = hooks.groupDocumentRecords(records);

  assert.equal(grouped.length, 1);
  assert.equal(grouped[0].is_mail_merge, true);
  assert.equal(grouped[0].document_type, 'mail_merge');
  assert.equal(grouped[0].recipient_count, 3);
  assert.equal(grouped[0].title, 'MURRELEKTRONIK 2022 - Welle 3 - GT - 12.10.2022');
  assert.deepEqual(grouped[0].record_ids, ['legacy_1', 'legacy_2', 'legacy_3']);
});

test('mail merge navigator loads recipient versions and resolves recipient search', async () => {
  const versions = [
    {
      id: 'merge_1_v2',
      document_id: 'merge_1',
      version: 2,
      source_kind: 'mail_merge_recipient',
      mail_merge_recipient: { id: 'person_2', label: 'Daniel Floris', index: 1, total: 3 },
    },
    {
      id: 'merge_1_v1',
      document_id: 'merge_1',
      version: 1,
      source_kind: 'mail_merge_recipient',
      mail_merge_recipient: { id: 'person_1', label: 'Bjarne Schäfer', index: 0, total: 3 },
    },
    {
      id: 'merge_1_v3',
      document_id: 'merge_1',
      version: 3,
      source_kind: 'mail_merge_recipient',
      mail_merge_recipient: { id: 'person_3', label: 'Markus H. Niedermayer', index: 2, total: 3 },
    },
  ];
  const state = {
    ctx: {
      db: {
        collection(name) {
          if (name !== 'document_versions') return null;
          return { find: () => ({ exec: async () => versions }) };
        },
      },
    },
    documents: [hooks.normalizeDocumentRecord({
      id: 'merge_1',
      title: 'MURRELEKTRONIK 2022',
      filename: 'murrel.docx',
      document_type: 'mail_merge',
      current_version_id: 'merge_1_v1',
      mail_merge: { recipient_count: 3 },
    })],
    selectedId: 'merge_1',
    selectedVersion: { id: 'merge_1_v2' },
  };

  const navigation = await hooks.refreshMailMergeNavigation(state);
  assert.deepEqual(navigation.entries.map(({ label }) => label), [
    'Bjarne Schäfer',
    'Daniel Floris',
    'Markus H. Niedermayer',
  ]);
  assert.equal(navigation.activeIndex, 1);
  assert.equal(hooks.findMailMergeRecipientIndex(navigation.entries, 'bjarne'), 0);
  assert.equal(hooks.findMailMergeRecipientIndex(navigation.entries, 'Niedermayer'), 2);
  assert.equal(hooks.findMailMergeRecipientIndex(navigation.entries, 'nicht vorhanden'), -1);
});

test('document management searches grouped recipient text and filters type, status, and provenance source', () => {
  const documents = [
    hooks.normalizeDocumentRecord({
      id: 'merge_1',
      title: 'Sommerkampagne',
      filename: 'sommer.docx',
      document_type: 'mail_merge',
      status: 'Created',
      index_text: 'Bjarne Schäfer Daniel Floris',
      provenance: { app_id: 'crm', source: 'campaign-mail-merge' },
      mail_merge: { recipient_count: 2 },
      updated_at_ms: 30,
    }),
    hooks.normalizeDocumentRecord({
      id: 'word_1',
      title: 'Alpha Angebot',
      filename: 'alpha.docx',
      document_type: 'word_document',
      status: 'Final',
      provenance: { app_id: 'documents', source: 'manual-report' },
      updated_at_ms: 10,
    }),
    hooks.normalizeDocumentRecord({
      id: 'note_1',
      title: 'Zeta Notiz',
      filename: 'zeta.md',
      document_type: 'markdown_document',
      status: 'Draft',
      updated_at_ms: 20,
    }),
  ];
  const state = {
    documents,
    searchQuery: 'schäfer',
    typeFilter: 'mail_merge',
    statusFilter: 'Created',
    appFilter: 'crm',
    sourceFilter: 'campaign-mail-merge',
    tagFilter: 'all',
    sortBy: 'updated_desc',
  };

  assert.deepEqual(hooks.visibleDocuments(state).map(({ id }) => id), ['merge_1']);
  assert.equal(hooks.documentFilterCount(state), 4);

  Object.assign(state, {
    searchQuery: '',
    typeFilter: 'all',
    statusFilter: 'all',
    appFilter: 'all',
    sourceFilter: 'all',
    sortBy: 'updated_asc',
  });
  assert.deepEqual(hooks.visibleDocuments(state).map(({ id }) => id), ['word_1', 'note_1', 'merge_1']);
  state.sortBy = 'title_asc';
  assert.deepEqual(hooks.visibleDocuments(state).map(({ id }) => id), ['word_1', 'merge_1', 'note_1']);
});

test('mail merge and series letter records use the DOCX render, save, and export path', () => {
  assert.equal(hooks.isDocxDocumentRecord({ document_type: 'word_document' }), true);
  assert.equal(hooks.isDocxDocumentRecord({ document_type: 'mail_merge' }), true);
  assert.equal(hooks.isDocxDocumentRecord({ document_type: 'series_letter' }), true);
  assert.equal(hooks.isDocxDocumentRecord({ document_type: 'markdown_document', filename: 'notes.md' }), false);
});

test('Documents UI exposes a real list resizer, collapsed actions drawer, and product name Dokumente', async () => {
  const [html, css, source, moduleJson, deMessages] = await Promise.all([
    readFile(new URL('./index.html', import.meta.url), 'utf8'),
    readFile(new URL('./index.css', import.meta.url), 'utf8'),
    readFile(new URL('./index.js', import.meta.url), 'utf8'),
    readFile(new URL('./module.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('./locales/de.json', import.meta.url), 'utf8').then(JSON.parse),
  ]);

  assert.match(html, /data-resize-frame/);
  assert.match(html, /class="ctox-column-resizer documents-library-resizer"/);
  assert.match(html, /data-resizer-var="--documents-library-width"/);
  assert.match(css, /\.documents-library-resizer[\s\S]*cursor:\s*col-resize/);
  assert.match(source, /new ResizeObserver/);
  assert.match(source, /root\.classList\.toggle\('is-compact', width <= 720\)/);
  assert.match(css, /\.documents-module\.is-compact \.documents-library-resizer[\s\S]*display:\s*none/);
  assert.match(html, /data-documents-actions-drawer[\s\S]*aria-hidden="true"[\s\S]*hidden/);
  assert.match(css, /\.documents-actions-drawer\s*\{[\s\S]*position:\s*absolute/);
  assert.match(source, /revisionedModuleAssetUrl\('\.\/index\.html'\)/);
  assert.match(source, /revisionedModuleAssetUrl\('\.\/index\.css'\)/);
  assert.equal(moduleJson.title, 'Dokumente');
  assert.equal(deMessages.documentsTitle, 'Dokumente');
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
