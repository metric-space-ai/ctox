import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createSyncRuntime } from './sync.js';

function fixture({ leader = false, readError = null, changeGeneration = false } = {}) {
  const browserToken = 'native-read-browser-token';
  const previousWindow = globalThis.window;
  globalThis.window = {
    location: { href: 'https://business-os.test/' },
    addEventListener() {}, removeEventListener() {},
  };
  const events = () => ({ subscribe() { return { unsubscribe() {} }; } });
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
    error$: events(), active$: events(), canceled$: events(), transportStatus$: events(),
    peerStates$: { ...events(), getValue() { return new Map(); } },
    async awaitInitialReplication() { return true; },
    async awaitQueryReady() { calls.ready++; },
    collectionQueryGenerationToken() { return generation; },
    getTransportStatus() { return {}; },
    async cancel() { calls.cancels++; },
  };
  const runtime = createSyncRuntime({
    db: {
      mode: 'rxdb', name: 'native-read-test', raw: { desktop_layout: collection },
      rxdb: {
        getMultiTabSyncCoordinator() { return coordinator; },
        getConnectionHandlerSimplePeer(options) { return options; },
        async replicateWebRTC() { calls.starts++; return state; },
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
  return { runtime, calls, document, async close() {
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
