import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');

test('shell reopens IndexedDB when a reload closes schema registration', () => {
  assert.match(appSource, /openBusinessDbAndRegisterCoreCollections\(dbName\)/);
  assert.match(appSource, /const maxAttempts = 3/);
  assert.match(appSource, /isIndexedDbConnectionClosingError\(error\) && attempt < maxAttempts/);
  assert.match(appSource, /await state\.db\?\.close\?\.\(\)/);
  assert.match(appSource, /state\.db = null/);
});
