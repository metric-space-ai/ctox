'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { ensureStartupFileConsumer, releaseStartupFileConsumer } = require('./business_os_startup_file_consumer.js');

test('absent shell waits without inventing transport or seeding records', () => {
  const scope = {};
  assert.deepEqual(ensureStartupFileConsumer(scope), { phase: 'waiting', collections: [] });
  assert.equal(scope.__ctoxStartupFileConsumer, undefined);
});

test('explicit consumer leases both file collections once and releases both', async () => {
  const acquired = [], released = [];
  const scope = { CTOX_BUSINESS_OS_APP: { sync: { async leaseCollection(collection, reason) {
    assert.equal(reason, 'startup-file-consumer');
    acquired.push(collection);
    return { collection, async release() { released.push(collection); } };
  } } } };
  assert.equal(ensureStartupFileConsumer(scope).phase, 'acquiring');
  ensureStartupFileConsumer(scope);
  await scope.__ctoxStartupFileConsumer.pending;
  assert.deepEqual(ensureStartupFileConsumer(scope), {
    phase: 'active', collections: ['desktop_files', 'desktop_file_chunks'],
  });
  assert.deepEqual(acquired, ['desktop_files', 'desktop_file_chunks']);
  assert.deepEqual(await releaseStartupFileConsumer(scope), { released: 2, failed: 0 });
  assert.deepEqual(released, acquired);
  ensureStartupFileConsumer(scope);
  assert.equal(acquired.length, 2, 'closed consumer cannot reacquire');
  assert.deepEqual(await releaseStartupFileConsumer(scope), { released: 0, failed: 0 });
});

test('partial acquisition failure is explicit and acquired lease is cleaned', async () => {
  let released = 0;
  const scope = { CTOX_BUSINESS_OS_APP: { sync: { async leaseCollection(collection) {
    if (collection === 'desktop_file_chunks') throw new Error('private transport details');
    return { collection, async release() { released++; } };
  } } } };
  ensureStartupFileConsumer(scope);
  await scope.__ctoxStartupFileConsumer.pending;
  assert.deepEqual(ensureStartupFileConsumer(scope), { phase: 'failed', collections: ['desktop_files'] });
  assert.equal(JSON.stringify(ensureStartupFileConsumer(scope)).includes('private'), false);
  assert.deepEqual(await releaseStartupFileConsumer(scope), { released: 1, failed: 0 });
  assert.equal(released, 1);
});

test('closing during acquisition releases the late lease without requesting another', async () => {
  let resolveLease, released = 0, calls = 0;
  const scope = { CTOX_BUSINESS_OS_APP: { sync: { leaseCollection() {
    calls++;
    return new Promise(resolve => { resolveLease = resolve; });
  } } } };
  ensureStartupFileConsumer(scope);
  assert.deepEqual(await releaseStartupFileConsumer(scope), { released: 0, failed: 0 });
  resolveLease({ collection: 'desktop_files', async release() { released++; } });
  await scope.__ctoxStartupFileConsumer.pending;
  assert.equal(calls, 1);
  assert.equal(released, 1);
  assert.equal(ensureStartupFileConsumer(scope).phase, 'closed');
});

test('release failure remains visible to the fixture', async () => {
  const scope = { CTOX_BUSINESS_OS_APP: { sync: { async leaseCollection(collection) {
    return { collection, async release() { throw new Error('failed release'); } };
  } } } };
  ensureStartupFileConsumer(scope);
  await scope.__ctoxStartupFileConsumer.pending;
  assert.deepEqual(await releaseStartupFileConsumer(scope), { released: 0, failed: 2 });
});
