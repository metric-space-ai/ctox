import assert from 'node:assert/strict';
import test from 'node:test';
import { CollectionSyncRegistry } from './sync-collection-registry.js';

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};

test('one lease follows replacement and ignores late completion of the old bridge', async () => {
  const registry = new CollectionSyncRegistry();
  const lease = registry.acquire('commands', 'test', async () => {});
  const old = deferred();
  const current = deferred();
  const seen = [];
  const subscription = lease.subscribeBridge(bridge => seen.push(bridge));
  registry.set('commands', old.promise);
  registry.delete('commands');
  registry.set('commands', current.promise);
  const live = { state: { generation: 2 } };
  current.resolve(live);
  await current.promise;
  old.resolve({ state: { generation: 1 } });
  await old.promise;
  assert.equal(lease.bridge, live);
  assert.equal(seen.at(-1), live);
  assert.equal(seen.some(bridge => bridge.state?.generation === 1), false);
  assert.equal(registry.leaseCount('commands'), 1);
  subscription.unsubscribe();
  await lease.release();
});

test('late startup rejection cannot poison a replacement bridge', async () => {
  const registry = new CollectionSyncRegistry();
  const lease = registry.acquire('commands', 'test', async () => {});
  const old = deferred();
  registry.set('commands', old.promise);
  const live = { state: { generation: 2 } };
  registry.set('commands', Promise.resolve(live));
  await Promise.resolve();
  old.reject(new Error('old failed'));
  await old.promise.catch(() => {});
  assert.equal(lease.bridge, live);
  await lease.release();
});

test('explicit stop revokes old leases without releasing a newly acquired lease', async () => {
  const registry = new CollectionSyncRegistry();
  let oldReleaseCalls = 0;
  const old = registry.acquire('commands', 'old', async () => { oldReleaseCalls++; });
  const seen = [];
  old.subscribeBridge(bridge => seen.push(bridge.mode));
  registry.revokeLeases('commands');
  registry.delete('commands');
  const current = registry.acquire('commands', 'current', async () => {});
  assert.equal(old.bridge.mode, 'stopped');
  assert.equal(seen.at(-1), 'stopped');
  assert.equal(await old.release(), false);
  assert.equal(oldReleaseCalls, 0);
  assert.equal(registry.leaseCount('commands'), 1);
  await current.release();
  assert.deepEqual(registry.leaseCounts(), []);
});

test('release notifies once, detaches observers, and preserves another active lease', async () => {
  const registry = new CollectionSyncRegistry();
  const remaining = [];
  const first = registry.acquire('commands', 'first', async count => remaining.push(count));
  const second = registry.acquire('commands', 'second', async count => remaining.push(count));
  const seen = [];
  first.subscribeBridge(bridge => seen.push(bridge.mode));
  assert.equal(await first.release(), true);
  assert.equal(first.bridge.mode, 'released');
  const eventsAfterRelease = seen.length;
  registry.set('commands', Promise.resolve({ mode: 'direct', state: {} }));
  await Promise.resolve();
  assert.equal(seen.length, eventsAfterRelease);
  assert.equal(await first.release(), false);
  assert.equal(await second.release(), true);
  assert.deepEqual(remaining, [1, 0]);
});


test('existing adapter assignments cannot override runtime ownership or revive a released lease', async () => {
  const registry = new CollectionSyncRegistry();
  const lease = registry.acquire('commands', 'adapter', async () => {});
  const old = { state: { generation: 1 } };
  registry.set('commands', Promise.resolve(old));
  await Promise.resolve();
  assert.doesNotThrow(() => { lease.bridge = old; });
  const pending = deferred();
  registry.set('commands', pending.promise);
  assert.doesNotThrow(() => { lease.bridge = { mode: 'pending', ready: pending.promise }; });
  assert.throws(() => { lease.bridge = old; }, { code: 'SYNC_LEASE_BRIDGE_NOT_CURRENT' });
  const current = { state: { generation: 2 } };
  pending.resolve(current);
  await pending.promise;
  assert.equal(lease.bridge, current);
  assert.throws(() => { lease.bridge = { state: {} }; }, { code: 'SYNC_LEASE_BRIDGE_NOT_CURRENT' });
  await lease.release();
  assert.throws(() => { lease.bridge = current; }, { code: 'SYNC_LEASE_BRIDGE_NOT_CURRENT' });
  assert.equal(lease.bridge.mode, 'released');
});

test('shutdown revokes all collection leases before later bridge promises settle', async () => {
  const registry = new CollectionSyncRegistry();
  const first = registry.acquire('a', 'first', async () => {});
  const second = registry.acquire('b', 'second', async () => {});
  const opening = deferred();
  registry.set('a', opening.promise);
  registry.revokeAllLeases();
  registry.clear();
  opening.resolve({ state: {} });
  await opening.promise;
  assert.equal(registry.size, 0);
  assert.deepEqual(registry.leaseCounts(), []);
  assert.equal(first.bridge.mode, 'stopped');
  assert.equal(second.bridge.mode, 'stopped');
});
