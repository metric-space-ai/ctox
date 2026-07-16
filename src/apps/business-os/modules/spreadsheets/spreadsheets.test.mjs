import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
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
  const blobChunks = {
    bulkUpsert: async (docs) => { bulkWrites.push(docs); },
    insert: async () => { throw new Error('spreadsheet_blob_chunks insert must not run per chunk'); },
  };
  const ctx = {
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
});
