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
    shellSource.indexOf('async function hydrateTaskbarPinsFromDesktopLayout('),
    shellSource.indexOf('async function withStartupTimeout(', shellSource.indexOf('async function hydrateTaskbarPinsFromDesktopLayout(')),
  );
  const hydrate = new Function(
    'state', 'console', 'scopedStorageKey', 'readScopedLocalStorage',
    'writeScopedLocalStorage', 'TASKBAR_PINS_KEY', 'TASKBAR_PIN_HYDRATION_TIMEOUT_MS',
    'decodeTaskbarPinCache', 'resolveTaskbarPinState', 'normalizeTaskbarPins',
    'encodeTaskbarPinCache', 'renderTabs', 'syncTaskbarPinsToDesktopLayout',
    'taskbarPinHydrationGeneration',
    `${hydrationSource}; const run = (runtimeState, generation = taskbarPinHydrationGeneration) => {
      state = runtimeState;
      return hydrateTaskbarPinsFromDesktopLayout(generation);
    }; run.setGeneration = (generation) => { taskbarPinHydrationGeneration = generation; };
    return run;`,
  )(
    null, { warn() {} }, key => key, readStorage(storage), write,
    'pins', 20000, decodeTaskbarPinCache, resolveTaskbarPinState,
    normalizeTaskbarPins, encodeTaskbarPinCache, () => {}, syncTaskbar, 0,
  );
  return hydrate;
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
function makeRetryHarness({
  nowMs = 0,
  baseDelayMs = 1000,
  windowMs = 60000,
  failuresBeforeSuccess = Number.POSITIVE_INFINITY,
  hydrate = null,
} = {}) {
  const clock = { now: nowMs };
  const timers = [];
  let nextTimerId = 1;
  let renderCount = 0;
  let cacheWrites = 0;
  let layoutWrites = 0;
  let hydrateCalls = 0;
  let state = {
    db: {}, sync: { readCollectionNativeDocument: async () => null }, modules: ['x'], taskbarPins: [], taskbarPinsKnown: false,
    taskbarPinsUpdatedAtMs: 0,
    taskbarPinHydrationRetryCount: 0,
    taskbarPinHydrationRetryStartedAtMs: 0,
    taskbarPinHydrationLastError: null,
  };
  const window = {
    clearTimeout(id) {
      const index = timers.findIndex((timer) => timer.id === id);
      if (index >= 0) timers.splice(index, 1);
    },
    setTimeout(callback, delay) {
      const id = nextTimerId++;
      timers.push({ id, callback, delay, runAt: clock.now + delay });
      return id;
    },
  };
  const defaultHydrate = async () => {
    hydrateCalls += 1;
    if (hydrateCalls <= failuresBeforeSuccess) {
      throw new Error(`authority unavailable (${hydrateCalls})`);
    }
    cacheWrites += 1;
    layoutWrites += 1;
    state.taskbarPins = ['remote'];
    state.taskbarPinsUpdatedAtMs = 500;
    state.taskbarPinsKnown = true;
  };
  const retrySource = shellSource.slice(
    shellSource.indexOf('function clearTaskbarPinHydrationRetry('),
    shellSource.indexOf('async function hydrateTaskbarPinsFromDesktopLayout(', shellSource.indexOf('function clearTaskbarPinHydrationRetry(')),
  );
  const start = new Function(
    'state', 'window', 'console', 'now', 'hydrateTaskbarPinsFromDesktopLayout', 'renderTabs',
    'TASKBAR_PIN_HYDRATION_RETRY_BASE_MS', 'TASKBAR_PIN_HYDRATION_RETRY_WINDOW_MS',
    `let taskbarPinHydrationRetryTimer = null;
     let taskbarPinHydrationRetryCount = 0;
     let taskbarPinHydrationRetryStartedAtMs = 0;
     let taskbarPinHydrationGeneration = 0;
     ${retrySource}
     return {
       schedule(runtimeState) {
         if (runtimeState) state = runtimeState;
         scheduleTaskbarPinHydrationRetry({ now });
       },
       reset(runtimeState) {
         if (runtimeState) state = runtimeState;
         clearTaskbarPinHydrationRetry({ resetAttempts: true });
       },
       get state() { return state; },
       get generation() { return taskbarPinHydrationGeneration; },
     };`,
  )(
    state, window, { warn() {} }, () => clock.now, hydrate || defaultHydrate,
    () => { renderCount += 1; }, baseDelayMs, windowMs,
  );

  const runDueTimers = async () => {
    const due = timers
      .filter((timer) => timer.runAt <= clock.now)
      .sort((left, right) => left.runAt - right.runAt);
    for (const timer of due) {
      const index = timers.findIndex((candidate) => candidate.id === timer.id);
      if (index >= 0) timers.splice(index, 1);
      timer.callback();
      await new Promise((resolve) => setImmediate(resolve));
      await new Promise((resolve) => setImmediate(resolve));
    }
  };
  return {
    clock,
    timers,
    get state() { return start.state; },
    get generation() { return start.generation; },
    get hydrateCalls() { return hydrateCalls; },
    get cacheWrites() { return cacheWrites; },
    get layoutWrites() { return layoutWrites; },
    get renderCount() { return renderCount; },
    schedule: (runtimeState) => start.schedule(runtimeState),
    reset: (runtimeState) => start.reset(runtimeState),
    async advanceTo(targetMs) {
      clock.now = Math.max(clock.now, targetMs);
      await runDueTimers();
    },
  };
}

// The window bounded retry chain converges on deterministic fake time, using
// the timer-execution clock rather than real 20 ms waits.
{
  const harness = makeRetryHarness({ baseDelayMs: 1, failuresBeforeSuccess: 4 });
  harness.schedule();
  for (let targetMs = 1; targetMs <= 15; targetMs += 1) {
    await harness.advanceTo(targetMs);
  }
  assert.equal(harness.hydrateCalls, 5);
  assert.equal(harness.state.taskbarPinsKnown, true);
  assert.deepEqual(harness.state.taskbarPins, ['remote']);
  assert.equal(harness.state.taskbarPinsUpdatedAtMs, 500);
  assert.equal(harness.state.taskbarPinHydrationRetryCount, 5);
  assert.equal(harness.state.taskbarPinHydrationLastError, null);
  assert.equal(harness.cacheWrites, 1);
  assert.equal(harness.layoutWrites, 1);
  assert.equal(harness.renderCount >= 1, true);
  assert.equal(harness.timers.length, 0, 'authority known stops the retry chain');
}

// A delayed timer checks expiry when it actually executes. Expiry leaves the
// strict authority unknown, performs no cache/layout mutation and stops.
{
  const harness = makeRetryHarness({ failuresBeforeSuccess: Number.POSITIVE_INFINITY });
  harness.schedule();
  while (harness.timers.length && harness.timers[0].runAt < 60000) {
    await harness.advanceTo(harness.timers[0].runAt);
  }
  await harness.advanceTo(60000);
  const callsAfterWindow = harness.hydrateCalls;
  assert.equal(callsAfterWindow > 0, true, 'expiry coverage must exercise retries');
  assert.equal(harness.state.taskbarPinsKnown, false, 'expired retries never fabricate authority');
  assert.deepEqual(harness.state.taskbarPins, []);
  assert.equal(harness.state.taskbarPinsUpdatedAtMs, 0);
  assert.equal(harness.state.taskbarPinHydrationRetryStartedAtMs, 0);
  assert.equal(typeof harness.state.taskbarPinHydrationLastError, 'string');
  assert.equal(harness.cacheWrites, 0, 'expired authority must not create a cache');
  assert.equal(harness.layoutWrites, 0, 'expired authority must not write layout');
  assert.equal(harness.timers.length, 0, 'expiry stops scheduling');
  await harness.advanceTo(120000);
  assert.equal(harness.hydrateCalls, callsAfterWindow, 'no retry survives the window');
}

// Reset fences a pending old-generation settlement. It can neither mutate the
// new session counters/error nor schedule another retry.
{
  const pendingReads = [];
  let hydrateCalls = 0;
  const harness = makeRetryHarness({
    hydrate: (generation) => {
      hydrateCalls += 1;
      return new Promise((resolve) => pendingReads.push({ generation, resolve }));
    },
  });
  const oldState = {
    db: { generation: 0 }, sync: { readCollectionNativeDocument: async () => null }, modules: ['x'], taskbarPins: [],
    taskbarPinsKnown: false, taskbarPinsUpdatedAtMs: 0,
    taskbarPinHydrationRetryCount: 0,
    taskbarPinHydrationRetryStartedAtMs: 0,
    taskbarPinHydrationLastError: null,
  };
  const newState = {
    db: { generation: 1 }, sync: { readCollectionNativeDocument: async () => null }, modules: ['x'], taskbarPins: ['new-session'],
    taskbarPinsKnown: false, taskbarPinsUpdatedAtMs: 0,
    taskbarPinHydrationRetryCount: 0,
    taskbarPinHydrationRetryStartedAtMs: 0,
    taskbarPinHydrationLastError: null,
  };
  harness.schedule(oldState);
  await harness.advanceTo(1000);
  assert.equal(hydrateCalls, 1);
  assert.equal(pendingReads[0].generation, 0);

  harness.reset(newState);
  assert.equal(harness.generation, 1);
  harness.schedule();
  await harness.advanceTo(2000);
  assert.equal(hydrateCalls, 2);
  assert.equal(pendingReads[1].generation, 1);

  // The old read settles while the new generation's read is pending.
  pendingReads[0].resolve();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(harness.state, newState);
  assert.deepEqual(harness.state.taskbarPins, ['new-session']);
  assert.equal(harness.state.taskbarPinsKnown, false);
  assert.equal(harness.state.taskbarPinsUpdatedAtMs, 0);
  assert.equal(harness.state.taskbarPinHydrationRetryCount, 1, 'old settlement must not alter the new attempt counter');
  assert.equal(harness.state.taskbarPinHydrationRetryStartedAtMs, 1000);
  assert.equal(harness.state.taskbarPinHydrationLastError, null);
  assert.equal(harness.timers.length, 0, 'an old settlement cannot schedule a retry');

  // The fenced new generation still converges normally.
  newState.taskbarPins = ['remote-new-session'];
  newState.taskbarPinsUpdatedAtMs = 500;
  newState.taskbarPinsKnown = true;
  pendingReads[1].resolve();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(harness.state.taskbarPinsKnown, true);
  assert.deepEqual(harness.state.taskbarPins, ['remote-new-session']);
  assert.equal(harness.renderCount >= 1, true);
  assert.equal(harness.timers.length, 0, 'known authority stops the new retry chain');
}

// The hydration body itself also fences generation, even when database and
// sync identity otherwise remain unchanged across a startup reset.
{
  let resolveNative;
  const sync = nativeSync(() => new Promise((resolve) => { resolveNative = resolve; }));
  const state = {
    db: {}, sync, modules: ['x'], taskbarPins: [], taskbarPinsKnown: false,
    taskbarPinsUpdatedAtMs: 0,
  };
  let cacheWrites = 0;
  let layoutWrites = 0;
  const hydrate = makeHydrate({
    write: () => { cacheWrites += 1; },
    syncTaskbar: async () => { layoutWrites += 1; },
  });
  const pending = hydrate(state, 0);
  hydrate.setGeneration(1);
  resolveNative({ toJSON: () => ({ taskbar_pins: ['old-session'], updated_at_ms: 999 }) });
  await pending;
  assert.deepEqual(state.taskbarPins, []);
  assert.equal(state.taskbarPinsKnown, false);
  assert.equal(state.taskbarPinsUpdatedAtMs, 0);
  assert.equal(cacheWrites, 0);
  assert.equal(layoutWrites, 0);
}


console.log('ok - authoritative taskbar pins preserve empty, pending and session states');
