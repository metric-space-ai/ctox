import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const syncSource = await readFile(new URL('../../shared/sync.js', import.meta.url), 'utf8');
const shellSource = await readFile(new URL('../../app.js', import.meta.url), 'utf8');

assert.match(syncSource, /async leaseModule\(moduleManifest/,
  'sync runtime must expose a scoped module lease');
assert.match(syncSource, /collectionLeaseCounts\.set\(normalized/,
  'collection bridges must be reference counted');
assert.match(syncSource, /remaining <= 0 && !pinnedCollections\.has\(normalized\)/,
  'the last app lease must stop only an unpinned collection bridge');
assert.match(syncSource, /resourceSnapshot\(\)/,
  'sync runtime must expose deterministic resource-budget evidence');

assert.match(shellSource, /activeModuleSyncLease/,
  'fullscreen navigation must retain its active module lease');
assert.match(shellSource, /window:closed[\s\S]{0,500}releaseModuleSyncLease\(\)/,
  'window close must release its module sync lease');
assert.doesNotMatch(shellSource, /syncStartedModules/,
  'the old permanent started-module set must not return');

console.log('module sync lifecycle smoke passed');
