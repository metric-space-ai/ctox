import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

import JSZip from '../../../../archive/reorg-review/templates/business-basic/apps/web/vendor/jszip.mjs';
import { mergeDocxFields } from '../vendor/document-format.mjs';

const fixture = new URL('../../../../tests/fixtures/office/document/edit-save.docx', import.meta.url);

test('DOCX export is byte-deterministic for identical input', async () => {
  const input = new Uint8Array(await readFile(fixture));
  const first = await mergeDocxFields(input, {}, { strict: false });
  await new Promise((resolve) => setTimeout(resolve, 1100));
  const second = await mergeDocxFields(input, {}, { strict: false });

  const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
  assert.equal(digest(first.bytes), digest(second.bytes));
  assert.deepEqual(first.bytes, second.bytes);

  const zip = await JSZip.loadAsync(first.bytes);
  const fileDates = Object.values(zip.files).filter(({ dir }) => !dir).map(({ date }) => date);
  assert.ok(fileDates.length > 0);
  assert.ok(fileDates.every((date) => date.getFullYear() === 1980));
});
