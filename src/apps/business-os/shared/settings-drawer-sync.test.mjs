import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('../app.js', import.meta.url), 'utf8');
const drawer = source.match(/async function openSettingsDrawer\(options = \{\}\) \{[\s\S]*?\n\}/)?.[0] || '';

assert.match(drawer, /sync:\s*createLiveSyncFacade\(\{ host: els\.rightDrawer \}\)/);
assert.doesNotMatch(drawer, /host:\s*hostEl/);

console.log('settings drawer sync host contract OK');
