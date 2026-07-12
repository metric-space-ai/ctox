import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');

function scopedCollections(moduleId) {
  const marker = `  ${moduleId}: Object.freeze([`;
  const start = appSource.indexOf(marker);
  assert.notEqual(start, -1, `${moduleId} must have a scoped system collection facade`);
  const end = appSource.indexOf('\n  ]),', start);
  assert.notEqual(end, -1, `${moduleId} scoped collection facade must terminate`);
  return new Set(
    [...appSource.slice(start, end).matchAll(/'([^']+)'/g)].map((match) => match[1]),
  );
}

for (const moduleId of ['documents', 'research']) {
  test(`${moduleId} system data facade covers every declared collection`, () => {
    const manifest = JSON.parse(readFileSync(new URL(`../modules/${moduleId}/module.json`, import.meta.url), 'utf8'));
    assert.deepEqual(
      [...scopedCollections(moduleId)].sort(),
      [...manifest.collections].sort(),
    );
  });
}
