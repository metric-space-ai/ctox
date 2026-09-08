import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import fs from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __spreadsheetsTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

test('spreadsheet chunk refresh does not re-query the file library or runbooks', async () => {
  const queried = [];
  const state = { spreadsheets: [], selectedId: '', ctx: {
    host: { querySelector: () => null },
    db: { collection(name) { queried.push(name); return { find: () => ({ exec: async () => [] }) }; } },
  } };
  await hooks.refreshSpreadsheetsFromLocal(state, new Set(['spreadsheet_blob_chunks']));
  assert.deepEqual(queried, []);
  await hooks.refreshSpreadsheetsFromLocal(state, new Set(['spreadsheets']));
  assert.deepEqual(queried, ['spreadsheets']);
});

test('spreadsheet background refresh does not render after disposal during a read', async () => {
  let active = true;
  let release;
  const held = new Promise(resolve => { release = resolve; });
  const state = { spreadsheets: [], selectedId: '', ctx: {
    host: { querySelector: () => assert.fail('disposed app must not render') },
    db: { collection() { return { find: () => ({ exec: () => held }) }; } },
  } };
  const pending = hooks.refreshSpreadsheetsFromLocal(state, new Set(['spreadsheets']), () => active);
  active = false;
  release([]);
  await pending;
});

test('spreadsheet version reads return locally without starting replication', async () => {
  const version = { id: 'local' };
  assert.equal(await hooks.resolveSpreadsheetVersionLocalFirst(async timeoutMs => {
    assert.equal(timeoutMs, 4500);
    return version;
  }, () => assert.fail('must not wait for sync')), version);
});

test('spreadsheet metadata deadline recovers once with a bounded network read', async () => {
  const budgets = [];
  let recoveries = 0;
  const version = { id: 'reconnected' };
  assert.equal(await hooks.resolveSpreadsheetVersionLocalFirst(async timeoutMs => {
    budgets.push(timeoutMs);
    if (budgets.length === 1) {
      return hooks.withSpreadsheetVersionTimeout(new Promise(() => {}), 1, 'deadline');
    }
    return version;
  }, async () => { recoveries += 1; }), version);
  assert.deepEqual(budgets, [4500, 60000]);
  assert.equal(recoveries, 1);
});

test('spreadsheet metadata recovery does not relabel integrity or permission failures', async () => {
  for (const code of ['permission_denied', 'blob_sha256_mismatch']) {
    const error = Object.assign(new Error(code), { code });
    assert.equal(hooks.isTransientSpreadsheetVersionReadError(error), false);
    await assert.rejects(hooks.resolveSpreadsheetVersionLocalFirst(
      () => hooks.withSpreadsheetVersionTimeout(Promise.reject(error), 100, 'deadline'),
      () => assert.fail('must not recover non-transient failure'),
    ), actual => actual === error);
  }
});

test('spreadsheet recovery resolves a pending direct bridge and waits only for its peer', async () => {
  const events = [];
  await hooks.awaitSpreadsheetVersionReplication({ sync: {
    async startCollection(name, options) {
      assert.equal(name, 'spreadsheet_versions');
      assert.deepEqual(options, { forceDirect: true });
      events.push('direct');
      return { ready: Promise.resolve({ state: {
        async waitForOpenPeerId(timeoutMs) {
          assert.equal(timeoutMs, 60000);
          events.push('native-peer');
          return 'native';
        },
        async awaitInitialReplication() { assert.fail('no full collection download'); },
        async awaitInSync() { assert.fail('no full collection synchronization'); },
      } }) };
    },
  } });
  assert.deepEqual(events, ['direct', 'native-peer']);
});

test('spreadsheet recovery propagates a refused direct channel without another read', async () => {
  let reads = 0;
  let starts = 0;
  const refused = new Error('forbidden');
  await assert.rejects(hooks.resolveSpreadsheetVersionLocalFirst(async () => { reads += 1; return null; },
    () => hooks.awaitSpreadsheetVersionReplication({ sync: {
      async startCollection() { starts += 1; throw refused; },
    } })), actual => actual === refused);
  assert.equal(reads, 1);
  assert.equal(starts, 1);
});

test('spreadsheet chrome is a two-pane file manager without a right runbook column', async () => {
  const [source, html, manifest] = await Promise.all([
    fs.readFile(new URL('./index.js', import.meta.url), 'utf8'),
    fs.readFile(new URL('./index.html', import.meta.url), 'utf8'),
    fs.readFile(new URL('./module.json', import.meta.url), 'utf8').then(JSON.parse),
  ]);
  assert.match(source, /data-spreadsheets-new[^\n]+[\s\S]{0,140}requestBlankSpreadsheet/);
  assert.match(source, /const BLANK_GRID_DATA/);
  assert.match(source, /state\.rightPaneEl\.hidden = true/);
  assert.doesNotMatch(html, /data-spreadsheets-head="runbooks"/);
  assert.doesNotMatch(html, /data-spreadsheets-toggle-actions/);
  assert.equal(manifest.layout.right, undefined);
});

test('spreadsheet runtime waits for initial replication before reading collections', async () => {
  const events = [];
  const ready = await hooks.ensureSpreadsheetRuntimeReady({
    actions: {
      async ensureRuntimeReady() {
        events.push('ready');
      },
    },
  });

  assert.equal(ready, true);
  assert.deepEqual(events, ['ready']);
  assert.equal(await hooks.ensureSpreadsheetRuntimeReady({}), false);
});

test('spreadsheet records without is_deleted remain visible', () => {
  assert.equal(hooks.isActiveSpreadsheetRecord({ id: 'sheet_1' }), true);
  assert.equal(hooks.isActiveSpreadsheetRecord({ id: 'sheet_1', is_deleted: false }), true);
  assert.equal(hooks.isActiveSpreadsheetRecord({ id: 'sheet_1', is_deleted: true }), false);
});

test('visibleSpreadsheets filters normalized rows by status, tag, search, and sort', () => {
  const state = {
    searchQuery: 'budget',
    statusFilter: 'Imported',
    tagFilter: 'finance',
    sortBy: 'title_asc',
    spreadsheets: [
      hooks.normalizeSpreadsheetRecord({
        id: 'sheet_2',
        title: 'Zeta Budget',
        filename: 'zeta.csv',
        status: 'Imported',
        tags: ['finance'],
        updated_at_ms: 20,
      }),
      hooks.normalizeSpreadsheetRecord({
        id: 'sheet_1',
        title: 'Alpha Budget',
        filename: 'alpha.csv',
        status: 'Imported',
        tags: ['finance'],
        updated_at_ms: 10,
      }),
      hooks.normalizeSpreadsheetRecord({
        id: 'sheet_3',
        title: 'Alpha Forecast',
        filename: 'forecast.csv',
        status: 'Draft',
        tags: ['finance'],
        updated_at_ms: 30,
      }),
    ],
  };

  assert.deepEqual(hooks.visibleSpreadsheets(state).map((record) => record.id), ['sheet_1', 'sheet_2']);
});

test('new spreadsheet validation requires a title before persistence', () => {
  assert.equal(hooks.validateNewSpreadsheetInput({ title: '' }).valid, false);
  assert.equal(hooks.validateNewSpreadsheetInput({ title: '  ' }).valid, false);
  assert.equal(hooks.validateNewSpreadsheetInput({ title: 'Budget 2026' }).valid, true);
});

test('import validation requires a supported spreadsheet file', () => {
  assert.equal(hooks.validateImportInput({ file: null }).valid, false);
  assert.equal(hooks.validateImportInput({ file: new File(['a,b'], 'budget.csv', { type: 'text/csv' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['a\tb'], 'budget.tsv', { type: 'text/tab-separated-values' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['PK'], 'budget.xlsx', { type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'notes.txt', { type: 'text/plain' }) }).valid, false);
});

test('browser-provided Research lineage is retained but cannot self-authorize factual tables', () => {
  const snapshotHash = `sha256:${'a'.repeat(64)}`;
  const ingestion = hooks.normalizeSpreadsheetIngestion({
    source_kind: 'research_generated',
    linked_records: [
      { kind: 'source_receipt', id: 'source-7', snapshot_hash: snapshotHash },
      { kind: 'claim', id: 'claim-7', evidence_id: 'evidence-7' },
    ],
    source_receipt_snapshot_hashes: [snapshotHash],
    knowledge_version: { version_id: 'knowledge-v7' },
  });

  assert.equal(ingestion.kind, 'research_generated');
  assert.equal(ingestion.valid, false);
  assert.match(ingestion.message, /confirmed provenance/i);
  assert.deepEqual(ingestion.linkedRecords, [
    { kind: 'source_receipt', id: 'source-7', snapshot_hash: snapshotHash },
    { kind: 'claim', id: 'claim-7', evidence_id: 'evidence-7' },
  ]);
  assert.deepEqual(ingestion.sourceReceiptSnapshotHashes, [snapshotHash]);
  assert.deepEqual(ingestion.knowledgeVersion, { version_id: 'knowledge-v7' });
});

test('Research spreadsheet ingestion fails closed when lineage is incomplete', () => {
  const ingestion = hooks.normalizeSpreadsheetIngestion({
    source_kind: 'research_generated',
    linked_records: [{ kind: 'claim', id: 'claim-without-receipt' }],
    knowledge_version: 'knowledge-v7',
  });

  assert.equal(ingestion.valid, false);
  assert.match(ingestion.message, /confirmed provenance/i);
  assert.throws(() => hooks.assertSpreadsheetIngestionAllowed(ingestion), (error) => {
    assert.equal(error.code, 'SPREADSHEET_LINEAGE_REQUIRED');
    return true;
  });
});

test('ordinary spreadsheet uploads remain explicit user imports', () => {
  const ingestion = hooks.normalizeSpreadsheetIngestion({
    filename: 'budget.csv',
    linked_records: [],
  });

  assert.equal(ingestion.kind, 'user_import');
  assert.equal(ingestion.valid, true);
  assert.deepEqual(ingestion.linkedRecords, []);
});

test('unresolved sourceFileId fails closed instead of becoming a user import', async () => {
  const sourceFiles = {
    findOne(id) {
      assert.equal(id, 'missing-source-file');
      return { exec: async () => null };
    },
  };
  const ingestion = await hooks.resolveSpreadsheetIngestion({
    ctx: {
      db: {
        collection(name) {
          assert.equal(name, 'desktop_files');
          return sourceFiles;
        },
      },
    },
  }, {
    sourceFileId: 'missing-source-file',
    filename: 'budget.csv',
  });

  assert.equal(ingestion.valid, false);
  assert.notEqual(ingestion.kind, 'user_import');
  assert.match(ingestion.message, /source file.*could not be resolved/i);
  assert.throws(() => hooks.assertSpreadsheetIngestionAllowed(ingestion), (error) => {
    assert.equal(error.code, 'SPREADSHEET_LINEAGE_REQUIRED');
    return true;
  });
});

test('file opening validates requested provenance before same-hash deduplication', async () => {
  const collectionCalls = [];
  const state = {
    spreadsheets: [{ id: 'existing-sheet', source_sha256: 'same-source-hash' }],
    ctx: {
      db: {
        collection(name) {
          collectionCalls.push(name);
          throw new Error(`deduplication should not read ${name}`);
        },
      },
    },
  };

  await assert.rejects(
    hooks.openSpreadsheetFile(state, {
      file: new File(['a,b\n1,2'], 'budget.csv', { type: 'text/csv' }),
      source_kind: 'research_generated',
    }),
    (error) => {
      assert.equal(error.code, 'SPREADSHEET_LINEAGE_REQUIRED');
      return true;
    },
  );
  assert.deepEqual(collectionCalls, []);
});

test('file-open deduplication reuses the imported spreadsheet with the same source hash', () => {
  const records = [
    { id: 'sheet_other', source_sha256: 'aaaa' },
    { id: 'sheet_loads', source_sha256: 'BEEF' },
  ];
  assert.equal(hooks.spreadsheetBySourceSha(records, 'beef')?.id, 'sheet_loads');
  assert.equal(hooks.spreadsheetBySourceSha(records, 'missing'), null);
});

test('supported records always use the real CTOX Office spreadsheet engine', () => {
  assert.equal(hooks.isOfficeSpreadsheetRecord({ filename: 'loads.csv', mime_type: 'text/csv' }), true);
  assert.equal(hooks.isOfficeSpreadsheetRecord({ filename: 'loads.xlsx' }), true);
  assert.equal(hooks.isOfficeSpreadsheetRecord({ filename: 'loads.tsv' }), true);
  assert.equal(hooks.isOfficeSpreadsheetRecord({ filename: 'model.json', mime_type: 'application/json' }), false);
});

test('malformed spreadsheet models normalize to a renderable grid', () => {
  const model = hooks.normalizeSpreadsheetModel({ data: [['A', 'B']] });
  assert.deepEqual(model.data, [['A', 'B']]);
  assert.equal(model.columns.length, 2);
});

test('CSV serialization quotes only when required, preserving numeric round-trip', () => {
  // Plain and numeric cells stay unquoted so their type survives re-import.
  assert.equal(hooks.escapeCsvCell(30), '30');
  assert.equal(hooks.escapeCsvCell('plain'), 'plain');
  assert.equal(hooks.escapeCsvCell(''), '');
  // Delimiters, quotes, newlines, and edge whitespace force quoting.
  assert.equal(hooks.escapeCsvCell('a,b'), '"a,b"');
  assert.equal(hooks.escapeCsvCell('a"b'), '"a""b"');
  assert.equal(hooks.escapeCsvCell('line1\nline2'), '"line1\nline2"');
  assert.equal(hooks.escapeCsvCell(' pad '), '" pad "');

  assert.equal(
    hooks.rowsToCsv([['Name', 'Total'], ['Acme, Inc', 30], ['', 'plain']]),
    'Name,Total\n"Acme, Inc",30\n,plain'
  );
});

test('spreadsheet blob chunks are persisted with one bulk write', async () => {
  const bulkWrites = [];
  let acknowledged = false;
  const blobChunks = {
    bulkUpsert: async (docs) => {
      bulkWrites.push(docs);
      return docs.map(row => ({ toJSON: () => ({ ...row, _meta: { lwt: Date.now() } }) }));
    },
    insert: async () => { throw new Error('spreadsheet_blob_chunks insert must not run per chunk'); },
  };
  const ctx = {
    sync: { async leaseCollection() { return { bridge: { state: {
      async waitForOpenPeerId() { return 'native'; },
      async pushDocumentsToPeer(_peer, rows) { assert.equal(rows.length, bulkWrites[0].length); acknowledged = true; },
    } }, async release() {} }; } },
    db: {
      collection(name) {
        if (name === 'spreadsheet_blob_chunks') return blobChunks;
        return {};
      },
    },
  };

  const bytes = new Uint8Array(260 * 1024);
  bytes.fill(67);
  await hooks.saveBlobChunks(ctx, {
    blobId: 'sheet_blob_bulk',
    spreadsheetId: 'sheet_bulk',
    versionId: 'sheet_version_bulk',
    mimeType: 'application/octet-stream',
    bytes,
  });

  assert.equal(bulkWrites.length, 1, 'blob chunks are written through one bulkUpsert call');
  assert.ok(bulkWrites[0].length > 1, 'test payload spans multiple chunk documents');
  assert.equal(acknowledged, true, 'source bytes are acknowledged before exposing references');
});

test('empty spreadsheet explorer shows syncing only while the collection is unready', () => {
  const unready = { collection: 'spreadsheets', state: 'catching-up', ready: false, syncing: true, updatedAt: 0 };
  const offlinePending = { collection: 'spreadsheets', state: 'offline-pending', ready: false, syncing: false, updatedAt: 0 };
  const live = { collection: 'spreadsheets', state: 'live', ready: true, syncing: false, updatedAt: 1 };

  // Empty + unready ⇒ syncing shell (render hint, includes offline-pending).
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [], spreadsheetsReadiness: unready }), true);
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [], spreadsheetsReadiness: offlinePending }), true);
  // Empty + ready ⇒ regular ctox-empty.
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [], spreadsheetsReadiness: live }), false);
  // Rows always win, regardless of readiness.
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [{ id: 'sheet_1' }], spreadsheetsReadiness: unready }), false);
  // No readiness signal (older shells/tests) keeps the previous empty behaviour.
  assert.equal(hooks.shouldRenderSpreadsheetsSyncing({ spreadsheets: [] }), false);
  // Fallback reads the canonical shell API when no snapshot is cached.
  assert.equal(
    hooks.shouldRenderSpreadsheetsSyncing({
      spreadsheets: [],
      ctx: { sync: { collectionReadiness: (name) => (name === 'spreadsheets' ? unready : null) } },
    }),
    true,
  );
});

test('spreadsheets context menu remains scoped to the mounted module host', async () => {
  const source = await fs.readFile(new URL('./index.js', import.meta.url), 'utf8');
  assert.match(source, /const moduleHost = state\.ctx\?\.host/);
  assert.match(source, /moduleHost\.append\(menu\)/);
  assert.doesNotMatch(source, /document\.body\.append\(menu\)/);
});
