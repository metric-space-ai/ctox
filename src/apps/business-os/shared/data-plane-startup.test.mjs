import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');

test('shell reopens IndexedDB only after a settled connection-closing error', () => {
  assert.match(appSource, /openBusinessDbAndRegisterCoreCollections\(dbName\)/);
  assert.match(appSource, /const maxAttempts = 3/);
  assert.match(
    appSource,
    /const retryable = isIndexedDbConnectionClosingError\(error\) && attempt < maxAttempts/,
  );
  assert.match(appSource, /await state\.db\?\.close\?\.\(\)/);
  assert.match(appSource, /state\.db = null/);
});

test('slow core registration is awaited without racing or closing its live IndexedDB', () => {
  const openStart = appSource.indexOf('async function openBusinessDbAndRegisterCoreCollections');
  const openEnd = appSource.indexOf('function isIndexedDbConnectionClosingError', openStart);
  const openBlock = appSource.slice(openStart, openEnd);
  const registerStart = appSource.indexOf('async function registerCoreCollections');
  const registerEnd = appSource.indexOf('async function primeWindowGeometryCache', registerStart);
  const registerBlock = appSource.slice(registerStart, registerEnd);

  assert.match(openBlock, /await traceShellPhase\(`core-schema-registration-\$\{attempt\}`, registerCoreCollections\)/);
  assert.doesNotMatch(openBlock, /CtoxCoreCollectionRegistrationTimeout|timeoutMs/);
  assert.match(registerBlock, /await state\.db\.addCollections\(consolidated\)/);
  assert.doesNotMatch(registerBlock, /Promise\.race|setTimeout|timeoutMs/);
});

function registrationFixture(registerCoreCollections) {
  const definition = appSource.match(/^async function openBusinessDbAndRegisterCoreCollections\(dbName\) \{[\s\S]*?^\}/m)?.[0];
  const classifier = appSource.match(/^function isIndexedDbConnectionClosingError\(error\) \{[\s\S]*?^\}/m)?.[0];
  assert.ok(definition && classifier, 'exercise the actual registration and error classifier');
  const state = { db: null };
  const counts = { opened: 0, closed: 0, delays: [] };
  const open = runInNewContext(`${definition}\n${classifier}\nopenBusinessDbAndRegisterCoreCollections`, {
    state,
    loadBusinessDbModule: async () => ({ createBusinessDb: async () => {
      counts.opened += 1;
      return { close: async () => { counts.closed += 1; } };
    } }),
    traceShellPhase: async (_, operation) => operation(),
    assertCriticalSyncCollectionsMatchBundle() {},
    setStartupProgress() {}, shellText: key => key,
    registerCoreCollections,
    window: { setTimeout(resolve, delay) { counts.delays.push(delay); queueMicrotask(resolve); } },
    console: { debug() {}, warn() {} },
  });
  return { open, state, counts };
}

test('actual startup keeps one live database while schema registration is pending', async () => {
  let finish;
  let registrations = 0;
  const f = registrationFixture(() => {
    registrations += 1;
    return new Promise(resolve => { finish = resolve; });
  });
  let settled = false;
  const pending = f.open('retained-workspace').then(() => { settled = true; });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(settled, false);
  assert.equal(registrations, 1);
  assert.equal(f.counts.opened, 1);
  assert.equal(f.counts.closed, 0);
  assert.deepEqual(f.counts.delays, []);
  const liveDb = f.state.db;
  finish();
  await pending;
  assert.equal(f.state.db, liveDb);
  assert.equal(f.counts.closed, 0);
});

test('actual startup retries a settled closing error and preserves other error identity', async () => {
  let registrations = 0;
  const closing = new Error('IDBDatabase database connection is closing');
  closing.name = 'InvalidStateError';
  const retry = registrationFixture(async () => {
    if (++registrations === 1) throw closing;
  });
  await retry.open('retained-workspace');
  assert.equal(registrations, 2);
  assert.equal(retry.counts.opened, 2);
  assert.equal(retry.counts.closed, 1);
  assert.deepEqual(retry.counts.delays, [150]);

  const unrelated = new Error('schema rejected');
  const failure = registrationFixture(async () => { throw unrelated; });
  await assert.rejects(failure.open('retained-workspace'), error => error === unrelated);
  assert.equal(failure.counts.opened, 1);
  assert.equal(failure.counts.closed, 1);
  assert.deepEqual(failure.counts.delays, []);
});

test('generic IndexedDB timeouts are not treated as schema corruption', () => {
  const classifierStart = appSource.indexOf('function isRxDbSchemaDriftError');
  const classifierEnd = appSource.indexOf('function hasLiveModulePreloadDataPlane', classifierStart);
  const classifierBlock = appSource.slice(classifierStart, classifierEnd);

  assert.match(classifierBlock, /RxDB Error-Code: DB6/);
  assert.match(classifierBlock, /previousSchemaHash/);
  assert.doesNotMatch(classifierBlock, /timed out|IndexedDB lock|open blocked/);
});
