import {
  CTOX_BUSINESS_OS_SCHEMA_HASHES,
  schemaHash,
} from '../dist/ctox-rxdb-js.mjs';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const testDir = dirname(fileURLToPath(import.meta.url));
const fixturePath = resolve(testDir, '../../../../core/business_os/business_os_schema_hashes.json');
const fixture = JSON.parse(readFileSync(fixturePath, 'utf8'));

const missing = [];
const stale = [];
for (const [collection, expected] of Object.entries(fixture)) {
  const actual = CTOX_BUSINESS_OS_SCHEMA_HASHES[collection];
  if (!actual) {
    missing.push(collection);
  } else if (actual !== expected) {
    stale.push({ collection, expected, actual });
  }
}

const extra = Object.keys(CTOX_BUSINESS_OS_SCHEMA_HASHES)
  .filter((collection) => !fixture[collection])
  .sort();

if (missing.length || stale.length || extra.length) {
  throw new Error(JSON.stringify({
    message: 'ctox-rxdb-js Business OS schema hash registry drifted from Rust fixture',
    missing,
    stale,
    extra,
  }, null, 2));
}

for (const [collection, expected] of Object.entries(fixture)) {
  const actual = await schemaHash({ version: 999, primaryKey: 'id', properties: { id: { type: 'string', maxLength: 64 } } }, collection);
  if (actual !== expected) {
    throw new Error(`schemaHash(${collection}) did not use registry hash`);
  }
}

console.log('ctox-rxdb-js schema hash registry smoke OK');
