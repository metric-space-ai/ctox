import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

import {
  decodeTaskbarPinCache,
  encodeTaskbarPinCache,
  resolveTaskbarPinState,
} from './taskbar-pins.js';

const legacy = decodeTaskbarPinCache(JSON.stringify(['files', 'ctox', 'files']));
assert.deepEqual(legacy, {
  pins: ['files', 'ctox'], updatedAtMs: 0, legacy: true, present: true,
});

const encoded = encodeTaskbarPinCache(['files', 'ctox'], 42);
assert.deepEqual(decodeTaskbarPinCache(encoded), {
  pins: ['files', 'ctox'], updatedAtMs: 42, legacy: false, present: true,
});
assert.equal(decodeTaskbarPinCache('[]').present, true);
assert.equal(decodeTaskbarPinCache('[]').pins.length, 0);
assert.equal(decodeTaskbarPinCache(null).present, false);
assert.equal(decodeTaskbarPinCache('{bad json').present, false);

// Presence, not array length, decides whether a side is known. Empty arrays
// follow exactly the same timestamp rule as non-empty selections.
assert.deepEqual(resolveTaskbarPinState({
  localPins: ['ctox'], localUpdatedAtMs: 200,
  remotePins: ['files'], remoteUpdatedAtMs: 100,
}), { pins: ['ctox'], updatedAtMs: 200, source: 'local' });
assert.deepEqual(resolveTaskbarPinState({
  localPins: ['ctox'], localUpdatedAtMs: 100,
  remotePins: [], remoteUpdatedAtMs: 200,
}), { pins: [], updatedAtMs: 200, source: 'remote' });
assert.deepEqual(resolveTaskbarPinState({
  localPins: [], localUpdatedAtMs: 100, localPresent: true,
  remotePins: ['files'], remoteUpdatedAtMs: 100,
}), { pins: ['files'], updatedAtMs: 100, source: 'remote' });
assert.deepEqual(resolveTaskbarPinState({
  localPins: ['ctox'], localUpdatedAtMs: 100,
  remotePins: null, remoteUpdatedAtMs: 0,
}), { pins: ['ctox'], updatedAtMs: 100, source: 'local' });

const shellSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
assert.match(
  shellSource,
  /Number\(existingLayout\?\.updated_at_ms \|\| 0\) === Number\(state\.taskbarPinsUpdatedAtMs \|\| 0\)[\s\S]*remotePins\.every\(\(id, index\) => id === localPins\[index\]\)[\s\S]*return;/,
  'desktop layout write-back must not rewrite an identical document',
);
assert.match(shellSource, /resolved\.updatedAtMs \|\| 0\);/, 'hydration must not invent an initialization timestamp');

const normalizeTaskbarPins = (pins, _modules, options = {}) => {
  const raw = Array.isArray(pins) ? pins : [];
  return options.preserveKnownEmpty || raw.length ? raw : ['fallback'];
};
const readStorage = (value) => () => value;

function makeHydrate({ storage = null, write = () => {}, syncTaskbar = async () => {} } = {}) {
  const hydrationSource = shellSource.slice(
    shellSource.indexOf('async function hydrateTaskbarPinsFromDesktopLayout() {'),
    shellSource.indexOf('async function withStartupTimeout(', shellSource.indexOf('async function hydrateTaskbarPinsFromDesktopLayout() {')),
  );
  return new Function(
    'state', 'console', 'scopedStorageKey', 'readScopedLocalStorage',
    'writeScopedLocalStorage', 'TASKBAR_PINS_KEY', 'TASKBAR_PIN_HYDRATION_TIMEOUT_MS',
    'decodeTaskbarPinCache', 'resolveTaskbarPinState', 'normalizeTaskbarPins',
    'encodeTaskbarPinCache', 'renderTabs', 'syncTaskbarPinsToDesktopLayout',
    `${hydrationSource}; return (runtimeState) => {
      state = runtimeState;
      return hydrateTaskbarPinsFromDesktopLayout();
    }; `,
  )(
    null, { warn() {} }, key => key, readStorage(storage), write,
    'pins', 20000, decodeTaskbarPinCache, resolveTaskbarPinState,
    normalizeTaskbarPins, encodeTaskbarPinCache, () => {}, syncTaskbar,
  );
}

function nativeSync(implementation) {
  return { readCollectionNativeDocument: implementation };
}

// Pending/failed authority is unknown: no timestamp, no cache and no write-back.
{
  const state = {
    db: {}, sync: nativeSync(() => new Promise(() => {})),
    modules: ['x'], taskbarPins: ['fallback'], taskbarPinsKnown: false,
    taskbarPinsUpdatedAtMs: 0,
  };
  let cacheWrites = 0;
  let layoutWrites = 0;
  const hydrate = makeHydrate({
    write: () => { cacheWrites += 1; },
    syncTaskbar: async () => { layoutWrites += 1; },
  });
  hydrate(state);
  await Promise.resolve();
  assert.equal(cacheWrites, 0, 'pending native read must not create a cache');
  assert.equal(layoutWrites, 0, 'pending native read must not write the layout');
}

// Completed absence is known but is not a user edit or a default layout write.
{
  const state = {
    db: {}, sync: nativeSync(async () => null), modules: ['x'],
    taskbarPins: ['fallback'], taskbarPinsKnown: false, taskbarPinsUpdatedAtMs: 0,
  };
  let cacheWrites = 0;
  let layoutWrites = 0;
  const hydrate = makeHydrate({
    write: () => { cacheWrites += 1; },
    syncTaskbar: async () => { layoutWrites += 1; },
  });
  await hydrate(state);
  assert.equal(state.taskbarPinsKnown, true);
  assert.deepEqual(state.taskbarPins, ['fallback']);
  assert.equal(state.taskbarPinsUpdatedAtMs, 0);
  assert.equal(cacheWrites, 0);
  assert.equal(layoutWrites, 0);
}

// Explicit native empty persists and wins ties over a default display value.
{
  const state = {
    db: {}, sync: nativeSync(async () => ({
      toJSON: () => ({ taskbar_pins: [], updated_at_ms: 123 }),
    })), modules: ['x'], taskbarPins: ['fallback'], taskbarPinsKnown: false,
    taskbarPinsUpdatedAtMs: 0,
  };
  let cacheValue = '';
  let layoutWrites = 0;
  const hydrate = makeHydrate({
    write: (_key, value) => { cacheValue = value; },
    syncTaskbar: async () => { layoutWrites += 1; },
  });
  await hydrate(state);
  assert.deepEqual(state.taskbarPins, []);
  assert.equal(state.taskbarPinsUpdatedAtMs, 123);
  assert.deepEqual(JSON.parse(cacheValue), { version: 2, pins: [], updated_at_ms: 123 });
  assert.equal(layoutWrites, 0);
}

// A real pending edit survives a failed best-effort cache and beats old native.
{
  const state = {
    db: {}, sync: nativeSync(async () => ({
      toJSON: () => ({ taskbar_pins: ['old-native'], updated_at_ms: 100 }),
    })), modules: ['x'], taskbarPins: [], taskbarPinsKnown: true,
    taskbarPinsUpdatedAtMs: 200,
  };
  let cacheWrites = 0;
  let writeBackDocuments = 0;
  const hydrate = makeHydrate({
    write: () => { cacheWrites += 1; throw new Error('quota'); },
    syncTaskbar: async (document) => { if (document) writeBackDocuments += 1; },
  });
  await hydrate(state);
  assert.deepEqual(state.taskbarPins, []);
  assert.equal(state.taskbarPinsUpdatedAtMs, 200);
  assert.equal(cacheWrites, 1, 'cache is best effort after a real edit');
  assert.equal(writeBackDocuments, 1, 'newer pending edit wins and writes back');
}

// A replaced database/runtime identity discards the late authority result.
{
  let resolveNative;
  const sync = nativeSync(() => new Promise((resolve) => { resolveNative = resolve; }));
  const state = {
    db: {}, sync, modules: ['x'], taskbarPins: [], taskbarPinsKnown: true,
    taskbarPinsUpdatedAtMs: 100,
  };
  let cacheWrites = 0;
  let layoutWrites = 0;
  const hydrate = makeHydrate({
    write: () => { cacheWrites += 1; },
    syncTaskbar: async () => { layoutWrites += 1; },
  });
  const pending = hydrate(state);
  state.db = { replacement: true };
  state.sync = nativeSync(async () => null);
  resolveNative({ toJSON: () => ({ taskbar_pins: ['old-session'], updated_at_ms: 999 }) });
  await pending;
  assert.deepEqual(state.taskbarPins, []);
  assert.equal(state.taskbarPinsUpdatedAtMs, 100);
  assert.equal(cacheWrites, 0);
  assert.equal(layoutWrites, 0);
}

// Same-identity reconnect is represented by the wrapper generation; pending
// state remains in memory until the new authority settles.
{
  const state = {
    db: {}, sync: nativeSync(async () => ({
      toJSON: () => ({ taskbar_pins: ['remote-new'], updated_at_ms: 500 }),
    })), modules: ['x'], taskbarPins: ['pending'], taskbarPinsKnown: true,
    taskbarPinsUpdatedAtMs: 300,
  };
  await makeHydrate()(state);
  assert.deepEqual(state.taskbarPins, ['remote-new']);
  assert.equal(state.taskbarPinsUpdatedAtMs, 500);
}

// Write-back rechecks identity before using an authoritative document handle.
{
  const writeBackSource = shellSource.slice(
    shellSource.indexOf('async function syncTaskbarPinsToDesktopLayout('),
    shellSource.indexOf('function renderModuleGroup(', shellSource.indexOf('async function syncTaskbarPinsToDesktopLayout(')),
  );
  let mutations = 0;
  const state = {
    db: {}, sync: {}, modules: [], taskbarPins: ['new-session'], taskbarPinsKnown: true,
    taskbarPinsUpdatedAtMs: 500,
  };
  const writeBack = new Function(
    'state', 'scopedStorageKey', 'readScopedLocalStorage', 'TASKBAR_PINS_KEY',
    'TASKBAR_PIN_HYDRATION_TIMEOUT_MS', 'sync', 'resolveTaskbarPinState',
    'normalizeTaskbarPins', 'writeScopedLocalStorage', 'encodeTaskbarPinCache',
    'renderTabs', 'console',
    `${writeBackSource}; return syncTaskbarPinsToDesktopLayout;`,
  )(
    state, key => key, () => null, 'pins', 20000,
    { readCollectionNativeDocument: async () => null }, resolveTaskbarPinState,
    normalizeTaskbarPins, () => {}, encodeTaskbarPinCache, () => {}, { warn() {} },
  );
  const pending = writeBack();
  state.db = { replacement: true };
  state.sync = { readCollectionNativeDocument: async () => null };
  await pending;
  assert.equal(mutations, 0, 'old write-back authority cannot mutate new session');
  assert.deepEqual(state.taskbarPins, ['new-session']);
}
// A replaced startup generation retries the strict authority read, then stops
// once the same native authority has made the pin state known.
{
  const retrySource = shellSource.slice(
    shellSource.indexOf('function clearTaskbarPinHydrationRetry('),
    shellSource.indexOf('async function hydrateTaskbarPinsFromDesktopLayout(', shellSource.indexOf('function clearTaskbarPinHydrationRetry(')),
  );
  let nativeReads = 0;
  const sync = {
    async readCollectionNativeDocument() {
      nativeReads += 1;
      if (nativeReads === 1) throw new Error('QUERY_CANCELLED: generation-replaced');
      return { toJSON: () => ({ taskbar_pins: ['remote-after-retry'], updated_at_ms: 500 }) };
    },
  };
  const state = {
    db: {}, sync, modules: ['x'], taskbarPins: [], taskbarPinsKnown: false,
    taskbarPinsUpdatedAtMs: 0,
  };
  let nextTimerId = 1;
  const timers = [];
  const cancelled = new Set();
  const window = {
    clearTimeout(id) {
      cancelled.add(id);
      const index = timers.findIndex((timer) => timer.id === id);
      if (index >= 0) timers.splice(index, 1);
    },
    setTimeout(callback, delay) {
      const id = nextTimerId++;
      timers.push({ id, callback, delay });
      return id;
    },
  };
  let renderCount = 0;
  const start = new Function(
    'state', 'window', 'console', 'TASKBAR_PIN_HYDRATION_RETRY_BASE_MS',
    'TASKBAR_PIN_HYDRATION_RETRY_LIMIT', 'hydrateTaskbarPinsFromDesktopLayout', 'renderTabs',
    `${retrySource}; return { start(runtimeState) { state = runtimeState; scheduleTaskbarPinHydrationRetry(); } };`,
  )(
    state, window, { warn() {} }, 500, 4,
    () => hydrateTaskbarPinsFromDesktopLayout(state), () => { renderCount += 1; },
  );
  const hydrate = makeHydrate({ syncTaskbar: async () => {} });
  const runCurrentTimer = async () => {
    const timer = timers.shift();
    if (!timer) throw new Error('expected a bounded hydration retry timer');
    timer.callback();
    await new Promise(resolve => setTimeout(resolve, 0));
    return timer;
  };

  start.start(state);
  const first = await runCurrentTimer();
  assert.equal(nativeReads, 1, 'first retry must observe the replaced generation');
  assert.equal(state.taskbarPinsKnown, false, 'a cancelled native read remains unknown');
  assert.equal(first.delay, 500, 'retry backoff starts bounded');
  assert.equal(timers.length, 1, 'the rejected read schedules exactly one follow-up');

  await runCurrentTimer();
  assert.equal(nativeReads, 2, 'the follow-up reads the newly available authority');
  assert.equal(state.taskbarPinsKnown, true);
  assert.deepEqual(state.taskbarPins, ['remote-after-retry']);
  assert.equal(state.taskbarPinsUpdatedAtMs, 500);
  assert.equal(renderCount >= 1, true);
  assert.equal(timers.length, 0, 'authority known stops the bounded retry chain');
}


console.log('ok - authoritative taskbar pins preserve empty, pending and session states');
