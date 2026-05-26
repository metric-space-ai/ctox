#!/usr/bin/env node
// Computes canonicalJson + fingerprint for every fixture under
// src/core/rxdb/tests/fixtures/query_fingerprint/ using the JS canonicalizer
// in src/apps/business-os/rxdb/src/query-fingerprint.mjs.
//
// Both JS and Rust tests then assert their runtime output matches these
// recorded values, which gives us byte-for-byte cross-language parity.
//
// Run: node src/core/rxdb/tools/regenerate_query_fingerprint_corpus.mjs

import { readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const corpusDir = resolve(here, '..', 'tests', 'fixtures', 'query_fingerprint');
const fingerprintModule = resolve(
  here,
  '..',
  '..',
  '..',
  'apps',
  'business-os',
  'rxdb',
  'src',
  'query-fingerprint.mjs',
);

const { canonicalQueryJson, queryFingerprint } = await import(fingerprintModule);

const entries = (await readdir(corpusDir)).filter((name) => name.endsWith('.json')).sort();
if (!entries.length) {
  console.error(`no fixtures found under ${corpusDir}`);
  process.exit(1);
}

let updated = 0;
for (const name of entries) {
  const path = join(corpusDir, name);
  const fixture = JSON.parse(await readFile(path, 'utf8'));
  if (!fixture?.input) {
    console.error(`${name}: missing 'input' key`);
    process.exit(1);
  }
  const canonicalJson = canonicalQueryJson(fixture.input);
  const fingerprint = await queryFingerprint(fixture.input);
  const next = { input: fixture.input, canonicalJson, fingerprint };
  const serialized = `${JSON.stringify(next, null, 2)}\n`;
  await writeFile(path, serialized);
  updated += 1;
  console.log(`${name}  ${fingerprint.slice(0, 16)}`);
}

console.log(`regenerated ${updated} fixture(s)`);
