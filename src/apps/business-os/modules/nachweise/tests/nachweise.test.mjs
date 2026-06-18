import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (rel) => readFileSync(resolve(moduleRoot, rel), 'utf8');

test('credentials: manifest is consistent and has no inline SVG', () => {
  const manifest = JSON.parse(read('module.json'));
  assert.equal(manifest.id, 'nachweise');
  assert.ok(manifest.collections.includes('business_credentials'));
  assert.ok(!manifest.layout || !manifest.layout.icon_svg, 'no inline SVG in manifest');
});

test('credentials: schema declares its owned collection', () => {
  const schemaSrc = read('schema.js');
  assert.match(schemaSrc, /business_credentials/);
  assert.match(schemaSrc, /export const collections/);
});
