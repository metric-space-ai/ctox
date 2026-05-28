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
  external: ['../../vendor/jspreadsheet.mjs'],
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __spreadsheetsTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

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
  assert.equal(hooks.validateImportInput({ file: new File(['[]'], 'budget.json', { type: 'application/json' }) }).valid, true);
  assert.equal(hooks.validateImportInput({ file: new File(['x'], 'notes.txt', { type: 'text/plain' }) }).valid, false);
});

test('malformed spreadsheet models normalize to a renderable grid', () => {
  const model = hooks.normalizeSpreadsheetModel({ data: [['A', 'B']] });
  assert.deepEqual(model.data, [['A', 'B']]);
  assert.equal(model.columns.length, 2);
});
