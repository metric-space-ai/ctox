import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import { test } from 'node:test';

const source = await readFile(new URL('../app.js', import.meta.url), 'utf8');
function definition(name) {
  const begin = source.indexOf(`async function ${name}(`);
  assert.ok(begin >= 0, `missing ${name}`);
  const end = source.indexOf('\n}\n', begin);
  assert.ok(end > begin, `missing end of ${name}`);
  return source.slice(begin, end + 2);
}
function synchronousDefinition(name) {
  const begin = source.indexOf(`function ${name}(`);
  assert.ok(begin >= 0, `missing ${name}`);
  const end = source.indexOf('\n}\n', begin);
  assert.ok(end > begin, `missing end of ${name}`);
  return source.slice(begin, end + 2);
}
function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
function fixture() {
  const registration = deferred();
  const geometry = deferred();
  const timers = new Map();
  const events = [];
  let id = 0;
  const context = vm.createContext({
    state: { db: { addCollections: () => registration.promise } },
    performance: { now: () => 1 },
    setStartupProgress: (percent) => events.push(percent),
    shellText: text => text,
    loadCoreSchemaModules: async () => ({ ctox: { collections: {} }, desktop: { collections: {} } }),
    withMigrationStrategies: value => value,
    primeWindowGeometryCache: () => { events.push('geometry'); return geometry.promise; },
    console: { log() {}, warn: message => events.push(message) },
    window: {
      setTimeout: (callback, ms) => { timers.set(++id, { callback, ms }); return id; },
      clearTimeout: key => timers.delete(key),
    },
  });
  vm.runInContext(`${definition('withStartupTimeout')}\n${definition('registerCoreCollections')}`, context);
  return { registration, geometry, timers, events, run: () => context.registerCoreCollections() };
}

test('schema registration is never raced or cancelled by the optional cache timeout', async () => {
  const f = fixture();
  let finished = false;
  const pending = f.run().then(() => { finished = true; });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.timers.size, 0);
  assert.equal(finished, false);
  assert.equal(f.events.includes('geometry'), false);
  f.registration.resolve();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.timers.size, 1);
  const timer = [...f.timers.values()][0];
  assert.equal(timer.ms, 1500);
  timer.callback();
  await pending;
  assert.equal(finished, true);
  assert.equal(f.timers.size, 0);
  f.geometry.resolve();
});

test('a ready geometry cache completes without waiting for its timeout', async () => {
  const f = fixture();
  f.registration.resolve();
  f.geometry.resolve();
  await f.run();
  assert.equal(f.timers.size, 0);
  assert.equal(f.events.filter(event => typeof event === 'string' && event.includes('timed out')).length, 0);
});

test('command transport warmup cannot block the cached workspace first paint', async () => {
  const openDataPlane = definition('openBusinessDataPlane');
  assert.match(openDataPlane, /startCommandTransportWarmup\(state\.sync\)/);
  assert.doesNotMatch(openDataPlane, /await state\.sync\.startCollection\('business_commands'\)/);

  const pending = deferred();
  const warnings = [];
  let starts = 0;
  const startWarmup = vm.runInNewContext(
    `${synchronousDefinition('startCommandTransportWarmup')}\nstartCommandTransportWarmup`,
    { console: { warn: (...args) => warnings.push(args) } },
  );
  const returned = startWarmup({ startCollection: () => { starts += 1; return pending.promise; } });
  assert.equal(returned, undefined);
  await Promise.resolve();
  assert.equal(starts, 1);
  assert.equal(warnings.length, 0);
  pending.resolve();
  await Promise.resolve();
  assert.equal(warnings.length, 0);
});
