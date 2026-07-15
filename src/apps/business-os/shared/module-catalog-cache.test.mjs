import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('../app.js', import.meta.url), 'utf8');
const loader = source.match(/async function loadPackagedModuleCatalog\(\)[\s\S]*?\n\}/)?.[0] || '';

assert.match(loader, /modules\/registry\.json/);
assert.match(loader, /cache: 'no-store'/);
assert.doesNotMatch(
  loader,
  /cache: 'force-cache'/,
  'runtime-installed module releases must not be hidden behind the shell build cache',
);

console.log('runtime module catalog cache contract OK');
