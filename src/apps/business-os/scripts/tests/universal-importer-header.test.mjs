import assert from 'node:assert/strict';
import { test } from 'node:test';
import { tabularCellsToCompanyRows } from '../../shared/universal-importer.js';

const firms = [['Weicon GmbH & Co. KG', 'Münster'], ['JOWAT Klebstoffe GmbH', 'Elsteraue']];
const names = (rows) => rows.map((row) => row.name);

test('a header below title rows is found and the title rows are not imported', () => {
  const rows = tabularCellsToCompanyRows([['Kundenliste Chemie Q3'], [], ['Firma', 'Ort'], ...firms]);
  assert.deepEqual(names(rows), ['Weicon GmbH & Co. KG', 'JOWAT Klebstoffe GmbH']);
});

test('common company column names are recognized, not taken for a company', () => {
  for (const header of ['Unternehmensname', 'Firmenname', 'Kunde', 'Account Name', 'Firmierung']) {
    const rows = tabularCellsToCompanyRows([[header, 'Ort'], ...firms]);
    assert.deepEqual(names(rows), ['Weicon GmbH & Co. KG', 'JOWAT Klebstoffe GmbH'], header);
  }
});

test('an ID column in front of the company name does not become the name', () => {
  const rows = tabularCellsToCompanyRows([['Nr.', 'Unternehmensname', 'Ort'], ['1', ...firms[0]], ['2', ...firms[1]]]);
  assert.deepEqual(names(rows), ['Weicon GmbH & Co. KG', 'JOWAT Klebstoffe GmbH']);
});
