import { readdir, readFile } from 'node:fs/promises';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  canonicalQueryJson,
  queryFingerprint,
} from '../dist/ctox-rxdb-js.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const corpusDir = resolve(
  here,
  '..',
  '..',
  '..',
  '..',
  'core',
  'rxdb',
  'tests',
  'fixtures',
  'query_fingerprint',
);

const fixtures = (await readdir(corpusDir)).filter((name) => name.endsWith('.json')).sort();
if (!fixtures.length) {
  throw new Error(`no fixtures found at ${corpusDir}`);
}

let ok = 0;
for (const name of fixtures) {
  const text = await readFile(join(corpusDir, name), 'utf8');
  const fixture = JSON.parse(text);
  const canonical = canonicalQueryJson(fixture.input);
  if (canonical !== fixture.canonicalJson) {
    throw new Error(`${name}: canonicalJson mismatch\nexpected: ${fixture.canonicalJson}\nactual:   ${canonical}`);
  }
  const fingerprint = await queryFingerprint(fixture.input);
  if (fingerprint !== fixture.fingerprint) {
    throw new Error(`${name}: fingerprint mismatch\nexpected: ${fixture.fingerprint}\nactual:   ${fingerprint}`);
  }
  ok += 1;
}
console.log(`ctox-rxdb-js query fingerprint corpus smoke OK (${ok} fixtures)`);
