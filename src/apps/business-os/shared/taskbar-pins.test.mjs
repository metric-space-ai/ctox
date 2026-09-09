import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import {
  decodeTaskbarPinCache,
  encodeTaskbarPinCache,
  resolveTaskbarPinState,
} from './taskbar-pins.js';

const legacy = decodeTaskbarPinCache(JSON.stringify(['files', 'ctox', 'files']));
assert.deepEqual(legacy, { pins: ['files', 'ctox'], updatedAtMs: 0, legacy: true });

const encoded = encodeTaskbarPinCache(['files', 'ctox'], 42);
assert.deepEqual(decodeTaskbarPinCache(encoded), {
  pins: ['files', 'ctox'],
  updatedAtMs: 42,
  legacy: false,
});

assert.deepEqual(resolveTaskbarPinState({
  localPins: ['ctox'],
  localUpdatedAtMs: 200,
  remotePins: ['files'],
  remoteUpdatedAtMs: 100,
}), { pins: ['ctox'], updatedAtMs: 200, source: 'local' });

assert.deepEqual(resolveTaskbarPinState({
  localPins: ['ctox'],
  localUpdatedAtMs: 100,
  remotePins: ['files'],
  remoteUpdatedAtMs: 200,
}), { pins: ['files'], updatedAtMs: 200, source: 'remote' });

const shellSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
assert.match(
  shellSource,
  /remoteUpdatedAtMs === Number\(state\.taskbarPinsUpdatedAtMs \|\| 0\)[\s\S]*remotePins\.every\(\(id, index\) => id === localPins\[index\]\)[\s\S]*return;/,
  'desktop layout hydration must not rewrite an identical document',
);

// A delayed native read is not an empty layout and must never publish defaults.
const hydrationSource = shellSource.slice(
  shellSource.indexOf('async function hydrateTaskbarPinsFromDesktopLayout() {'),
  shellSource.indexOf('async function withStartupTimeout(', shellSource.indexOf('async function hydrateTaskbarPinsFromDesktopLayout() {')),
);
let resolveLayout;
const pendingLayout = new Promise(resolve => { resolveLayout = resolve; });
let cacheWrites = 0;
let syncWrites = 0;
const hydrationState = {
  db: { collection: () => ({ findOne: () => ({ exec: () => pendingLayout }) }) },
  modules: ['ctox'], taskbarPins: ['ctox'], taskbarPinsUpdatedAtMs: 0,
};
const hydrate = new Function(
  'state', 'withStartupTimeout', 'decodeTaskbarPinCache', 'readScopedLocalStorage',
  'TASKBAR_PINS_KEY', 'resolveTaskbarPinState', 'normalizeTaskbarPins',
  'writeScopedLocalStorage', 'encodeTaskbarPinCache', 'syncTaskbarPinsToDesktopLayout',
  `${hydrationSource}; return hydrateTaskbarPinsFromDesktopLayout;`,
)(hydrationState, async (_promise, _timeout, fallback) => fallback,
  decodeTaskbarPinCache, () => null, 'pins', resolveTaskbarPinState,
  pins => pins, () => { cacheWrites++; }, encodeTaskbarPinCache,
  async () => { syncWrites++; });
const hydration = hydrate();
await Promise.resolve();
await Promise.resolve();
assert.equal(cacheWrites, 0, 'pending layout read must not invent a cache timestamp');
assert.equal(syncWrites, 0, 'pending layout read must not publish fallback pins');
resolveLayout({ toJSON: () => ({ taskbar_pins: ['files'], updated_at_ms: 123 }) });
await hydration;
assert.deepEqual(hydrationState.taskbarPins, ['files']);
assert.equal(hydrationState.taskbarPinsUpdatedAtMs, 123);
assert.equal(syncWrites, 1);

// Session replacement while the read is in flight must not receive old pins.
let resolveOldSession;
const oldRead = new Promise(resolve => { resolveOldSession = resolve; });
hydrationState.db = { collection: () => ({ findOne: () => ({ exec: () => oldRead }) }) };
const oldHydration = hydrate();
hydrationState.db = {};
const writesBeforeOldRead = cacheWrites;
resolveOldSession({ toJSON: () => ({ taskbar_pins: ['old-session'], updated_at_ms: 999 }) });
await oldHydration;
assert.equal(cacheWrites, writesBeforeOldRead);
assert.equal(syncWrites, 1);
assert.deepEqual(hydrationState.taskbarPins, ['files']);

// The final write-back read has the same session boundary as hydration.
const writeBackSource = shellSource.slice(
  shellSource.indexOf('async function syncTaskbarPinsToDesktopLayout() {'),
  shellSource.indexOf('function renderModuleGroup(', shellSource.indexOf('async function syncTaskbarPinsToDesktopLayout() {')),
);
let resolveWriteBack;
const pendingWriteBack = new Promise(resolve => { resolveWriteBack = resolve; });
let staleMutations = 0;
const writeBackState = {
  db: { collection: () => ({ findOne: () => ({ exec: () => pendingWriteBack }) }) },
  modules: [], taskbarPins: ['new-session'], taskbarPinsUpdatedAtMs: 500,
};
const writeBack = new Function('state', 'normalizeTaskbarPins',
  'writeScopedLocalStorage', 'TASKBAR_PINS_KEY', 'encodeTaskbarPinCache', 'renderTabs',
  `${writeBackSource}; return syncTaskbarPinsToDesktopLayout;`,
)(writeBackState, pins => pins, () => { staleMutations++; }, 'pins',
  encodeTaskbarPinCache, () => { staleMutations++; });
const pendingSync = writeBack();
writeBackState.db = {};
resolveWriteBack({
  toJSON: () => ({ taskbar_pins: ['old-session'], updated_at_ms: 999 }),
  incrementalPatch: async () => { staleMutations++; },
});
await pendingSync;
assert.equal(staleMutations, 0, 'old write-back read must not mutate the new session');
assert.deepEqual(writeBackState.taskbarPins, ['new-session']);

console.log('ok - taskbar pins survive reloads and newest-write-wins reconciliation');
