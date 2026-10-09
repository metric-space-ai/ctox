import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createSyncRuntime, __ctoxSyncTestHooks } from './sync.js';

function fixture({ leader = false, readError = null, changeGeneration = false, startup = null,
  cancelGate = null, onCancel = () => {}, onStart = () => {} } = {}) {
  const browserToken = 'native-read-browser-token';
  const previousWindow = globalThis.window;
  globalThis.window = {
    location: { href: 'https://business-os.test/' },
    addEventListener() {}, removeEventListener() {},
  };
  const events = (initial) => {
    const listeners = new Set();
    return {
      subscribe(listener) {
        listeners.add(listener);
        if (initial !== undefined) listener(initial);
        return { unsubscribe() { listeners.delete(listener); } };
      },
      emit(value) { for (const listener of listeners) listener(value); },
    };
  };
  const coordinator = {
    onRoleChange() { return () => {}; }, onDirty() { return () => {}; },
    async start() { return this.snapshot(); },
    isLeader() { return leader; },
    snapshot() { return { isLeader: leader, role: leader ? 'leader' : 'follower' }; },
    stop() {},
  };
  const calls = { starts: 0, cancels: 0, ready: 0, queries: [] };
  let generation = 'native-generation-1';
  const document = { id: 'layout-1', pins: ['crew'] };
  const collection = {
    name: 'desktop_layout',
    schema: { version: 0, primaryPath: 'id', async hash() { return 'layout-hash'; } },
    storageCollection: {}, observe() { return () => {}; },
    findOne(query) {
      calls.queries.push(query);
      return { async exec() {
        if (readError) throw readError;
        if (changeGeneration) generation = 'native-generation-2';
        return document;
      } };
    },
  };
  const state = {
    collection, activeRemotePeerId: 'native',
    error$: events(), active$: events(true), canceled$: events(), transportStatus$: events(),
    peerStates$: { ...events(), getValue() { return new Map(); } },
    async awaitInitialReplication() { return true; },
    async awaitQueryReady() { calls.ready++; },
    collectionQueryGenerationToken() { return generation; },
    getTransportStatus() { return {}; },
    async cancel() { calls.cancels++; onCancel(); if (cancelGate) await cancelGate; },
  };
  const runtime = createSyncRuntime({
    db: {
      mode: 'rxdb', name: 'native-read-test', raw: { desktop_layout: collection },
      rxdb: {
        getMultiTabSyncCoordinator() { return coordinator; },
        getConnectionHandlerSimplePeer(options) { return options; },
        async replicateWebRTC() { calls.starts++; onStart(); if (startup) await startup; return state; },
      },
    },
    config: {
      transport: 'webrtc', sync_room: 'ctox-business-os:native-read-test',
      signaling_urls: ['wss://signal.test/room'],
      signaling_auth_version: 'ctox-role-bound-v1',
      signaling_browser_token: browserToken,
      signaling_browser_token_hash: createHash('sha256').update(browserToken).digest('hex'),
      signaling_native_token_hash: createHash('sha256').update('native-token').digest('hex'),
    },
  });
  return { runtime, calls, document, state, async close() {
    await runtime.stop(); globalThis.window = previousWindow;
  } };
}

for (const leader of [false, true]) {
  test(`authoritative read reaches native state in a ${leader ? 'leader' : 'follower'} tab`, async () => {
    const f = fixture({ leader });
    try {
      const ordinary = await f.runtime.leaseCollection('desktop_layout', 'open-window');
      assert.equal(f.calls.starts, leader ? 1 : 0);
      const result = await f.runtime.readCollectionNativeDocument('desktop_layout', 'layout-1');
      assert.strictEqual(result, f.document);
      assert.equal(f.calls.starts, 1);
      assert.equal(f.calls.ready, 1);
      assert.deepEqual(f.calls.queries[0].selector, { id: 'layout-1' });
      assert.match(f.calls.queries[0].requireRevision, /^authority:desktop_layout:layout-1:/);
      assert.equal(f.calls.cancels, 0, 'reader release must preserve the window lease');
      const another = await f.runtime.leaseCollection('desktop_layout', 'another-window');
      assert.strictEqual(another.bridge.state, ordinary.bridge.state);
      assert.equal(f.calls.starts, 1, 'ordinary acquisition reuses the direct bridge');
      await another.release();
      await ordinary.release();
      assert.equal(f.calls.cancels, 1);
    } finally { await f.close(); }
  });
}

test('failed authoritative read releases its direct bridge without returning cached data', async () => {
  const f = fixture({ readError: new Error('native unavailable') });
  try {
    await assert.rejects(f.runtime.readCollectionNativeDocument('desktop_layout', 'layout-1'), /native unavailable/);
    assert.equal(f.calls.cancels, 1);
  } finally { await f.close(); }
});

test('authoritative read rejects a changed native generation', async () => {
  const f = fixture({ changeGeneration: true });
  try {
    await assert.rejects(f.runtime.readCollectionNativeDocument('desktop_layout', 'layout-1'), /generation.*changed/);
    assert.equal(f.calls.cancels, 1);
  } finally { await f.close(); }
});

test('a timed-out native read releases a late lease without cancelling another owner', async () => {
  let finishStartup;
  const startup = new Promise((resolve) => { finishStartup = resolve; });
  const f = fixture({ startup });
  try {
    const ordinary = await f.runtime.leaseCollection('desktop_layout', 'open-window');
    await assert.rejects(
      f.runtime.readCollectionNativeDocument('desktop_layout', 'layout-1', { timeoutMs: 250 }),
      /Native read lease.*exceeded 250ms/,
    );
    finishStartup();
    await f.runtime.startCollection('desktop_layout', { pin: false, forceDirect: true });
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(f.calls.cancels, 0, 'late reader cleanup must preserve the window lease');
    assert.equal(f.calls.queries.length, 0, 'expired reader must not issue a query');
    await ordinary.release();
    assert.equal(f.calls.cancels, 1, 'no late reader lease may retain the bridge');
  } finally { finishStartup(); await f.close(); }
});

test('closing the last window while repair is stopping cannot resurrect its unowned bridge', async () => {
  let finishCancel, enteredCancel;
  const cancelGate = new Promise(resolve => { finishCancel = resolve; });
  const cancelling = new Promise(resolve => { enteredCancel = resolve; });
  const f = fixture({ leader: true, cancelGate, onCancel: enteredCancel });
  try {
    const lease = await f.runtime.leaseCollection('desktop_layout', 'window-under-repair');
    const repairing = f.runtime.restartCollection('desktop_layout');
    await cancelling;
    await lease.release();
    finishCancel();
    await repairing;
    const resources = f.runtime.resourceSnapshot();
    assert.deepEqual(resources.activeCollections, []);
    assert.deepEqual(resources.bridgeCollections, []);
    assert.deepEqual(resources.leaseCounts, {});
    assert.equal(f.calls.starts, 1, 'an expired repair must not open a new native bridge');
  } finally { finishCancel(); await f.close(); }
});


test('releasing the last active lease clears diagnostics used by heartbeat repair', async () => {
  const f = fixture({ leader: true });
  try {
    const lease = await f.runtime.leaseCollection('desktop_layout', 'last-window');
    assert.equal(f.runtime.diagnostics.collections.desktop_layout.active, true);
    await lease.release();
    assert.equal(f.runtime.diagnostics.collections.desktop_layout.active, false);
    assert.equal(f.runtime.diagnostics.collections.desktop_layout.connectionStatus, 'stopped');
    assert.deepEqual(__ctoxSyncTestHooks.repairCandidateCollectionNames(
      new Set(f.runtime.resourceSnapshot().activeCollections), f.runtime.diagnostics.collections,
    ), []);
    assert.equal(f.calls.cancels, 1);
  } finally { await f.close(); }
});

test('closing the last window during a batch repair leaves no bridge to retry', async () => {
  let finishCancel, enteredCancel;
  const cancelGate = new Promise(resolve => { finishCancel = resolve; });
  const cancelling = new Promise(resolve => { enteredCancel = resolve; });
  const f = fixture({ leader: true, cancelGate, onCancel: enteredCancel });
  try {
    const lease = await f.runtime.leaseCollection('desktop_layout', 'last-window');
    const repairing = f.runtime.restartCollections(['desktop_layout']);
    await cancelling;
    await lease.release();
    finishCancel();
    assert.deepEqual(await repairing, []);
    assert.deepEqual(f.runtime.resourceSnapshot(), {
      activeCollections: [], bridgeCollections: [], pinnedCollections: [], leaseCounts: {},
    });
    assert.equal(f.calls.starts, 1);
  } finally { finishCancel(); await f.close(); }
});

test('a remaining window keeps its bridge after another window closes during repair', async () => {
  let finishCancel, enteredCancel;
  const cancelGate = new Promise(resolve => { finishCancel = resolve; });
  const cancelling = new Promise(resolve => { enteredCancel = resolve; });
  const f = fixture({ leader: true, cancelGate, onCancel: enteredCancel });
  try {
    const first = await f.runtime.leaseCollection('desktop_layout', 'first-window');
    const second = await f.runtime.leaseCollection('desktop_layout', 'second-window');
    const repairing = f.runtime.restartCollection('desktop_layout');
    await cancelling;
    await first.release();
    finishCancel();
    const repaired = await repairing;
    assert.equal(f.calls.starts, 2);
    assert.strictEqual(second.bridge, repaired);
    assert.deepEqual(f.runtime.resourceSnapshot().leaseCounts, { desktop_layout: 1 });
    await second.release();
    assert.deepEqual(f.runtime.resourceSnapshot().bridgeCollections, []);
    assert.equal(f.calls.cancels, 2);
  } finally { finishCancel(); await f.close(); }
});

test('an explicit stop of the last pin cannot be undone by an older repair', async () => {
  let finishCancel, enteredCancel;
  const cancelGate = new Promise(resolve => { finishCancel = resolve; });
  const cancelling = new Promise(resolve => { enteredCancel = resolve; });
  const f = fixture({ leader: true, cancelGate, onCancel: enteredCancel });
  try {
    await f.runtime.startCollection('desktop_layout');
    const repairing = f.runtime.restartCollection('desktop_layout');
    await cancelling;
    await f.runtime.stopCollection('desktop_layout');
    finishCancel();
    assert.equal((await repairing).mode, 'stopped');
    assert.equal(f.calls.starts, 1);
    assert.deepEqual(f.runtime.resourceSnapshot().pinnedCollections, []);
  } finally { finishCancel(); await f.close(); }
});

test('a late native bridge is cancelled once and cannot republish active diagnostics', async () => {
  let finishStartup, enteredStartup;
  const startup = new Promise(resolve => { finishStartup = resolve; });
  const starting = new Promise(resolve => { enteredStartup = resolve; });
  const f = fixture({ leader: true, startup, onStart: enteredStartup });
  try {
    const acquisition = f.runtime.leaseCollection('desktop_layout', 'closing-window');
    await starting;
    await f.runtime.stopCollection('desktop_layout');
    finishStartup();
    const lease = await acquisition;
    await new Promise(resolve => setImmediate(resolve));
    f.state.active$.emit(true);
    assert.equal(f.calls.cancels, 1, 'the bounded stop and late cleanup share one cancellation');
    assert.equal(f.runtime.diagnostics.collections.desktop_layout.active, false);
    assert.deepEqual(f.runtime.resourceSnapshot(), {
      activeCollections: [], bridgeCollections: [], pinnedCollections: [], leaseCounts: {},
    });
    await lease.release();
    assert.equal(f.calls.cancels, 1);
  } finally { finishStartup(); await f.close(); }
});
