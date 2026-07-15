import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('../../app.js', import.meta.url), 'utf8');
const facadeMatch = source.match(/function createLiveSyncFacade\(\{ host = null \} = \{\}\) \{[\s\S]*?\n\}/);

assert.ok(facadeMatch, 'createLiveSyncFacade exists');
assert.match(
  facadeMatch[0],
  /leaseCollection:\s*async\s*\(\.\.\.args\)[\s\S]*state\.sync\?\.leaseCollection\?\.\(\.\.\.args\)/,
  'module ctx.sync exposes mount-bounded scoped collection leases',
);
assert.match(
  facadeMatch[0],
  /startCollection:[\s\S]*pin:\s*false/,
  'module ctx.sync eager starts never promote app bridges to shell pins',
);
assert.match(
  facadeMatch[0],
  /!host\.isConnected[\s\S]*lease\?\.release/,
  'late leases are released when their module host has closed',
);

const settingsDrawerMatch = source.match(/async function openSettingsDrawer\(options = \{\}\) \{[\s\S]*?\n\}/);
assert.ok(settingsDrawerMatch, 'settings drawer entrypoint exists');
assert.match(
  settingsDrawerMatch[0],
  /sync:\s*createLiveSyncFacade\(\{ host: els\.rightDrawer \}\)/,
  'settings drawer sync facade is bounded to the actual drawer host',
);
assert.doesNotMatch(
  settingsDrawerMatch[0],
  /host:\s*hostEl/,
  'settings drawer must not reference the module-only hostEl binding',
);
