import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('./index.js', import.meta.url), 'utf8');

assert.match(source, /commandBus\.dispatch\([\s\S]*\{ until: 'accepted' \}\)/);
assert.doesNotMatch(
  source.match(/async function startBrowserRuntimeSync[\s\S]*?\n\}/)?.[0] || '',
  /catch\s*\([^)]*\)\s*\{[\s\S]*console\.warn/,
  'browser sync startup errors must remain visible to the caller',
);

console.log('browser start command contract OK');
