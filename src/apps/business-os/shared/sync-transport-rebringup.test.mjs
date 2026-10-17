// Incident 15.10.2026 (welsch): after a network switch every WebRTC
// connection dropped. The background repair restarted only the unhealthy
// collection and re-registered it on the SAME shared room peer, which the
// still-registered healthy collections kept alive. That transport never
// reopened, so the individual restart + individual retry failed with
// peer_connect_timeout and only another deferred repair was scheduled.
//
// This test models the shared room peer like replication-webrtc.mjs does
// (SharedRoomPeer: one transport per room, closed when the LAST registration
// leaves, recreated on the next registration) and proves that a failed
// individual restart immediately triggers a full transport re-bring-up.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createSyncRuntime, __ctoxSyncTestHooks } from './sync.js';

const {
  shouldEscalateToTransportRebringup,
  transportRebringupBackoffMs,
  REPAIR_CYCLE_OPEN_TIMEOUT_MS,
} = __ctoxSyncTestHooks;

function events(initial) {
  const listeners = new Set();
  return {
    subscribe(listener) {
      listeners.add(listener);
      if (initial !== undefined) listener(initial);
      return { unsubscribe() { listeners.delete(listener); } };
    },
    emit(value) { for (const listener of listeners) listener(value); },
  };
}

function createRoomModel() {
  // One shared transport per room, as SharedRoomPeer in replication-webrtc.mjs.
  const room = {
    transport: null,
    transportsCreated: 0,
    transportsClosed: 0,
    registrations: new Set(),
    // When false, a freshly created transport opens (network is healthy).
    nextTransportDead: false,
  };
  room.register = (state) => {
    if (!room.transport) {
      room.transportsCreated += 1;
      room.transport = { id: room.transportsCreated, dead: room.nextTransportDead, closed: false };
    }
    room.registrations.add(state);
    return room.transport;
  };
  room.unregister = (state) => {
    if (!room.registrations.delete(state)) return;
    if (room.registrations.size === 0 && room.transport) {
      room.transport.closed = true;
      room.transport = null;
      room.transportsClosed += 1;
    }
  };
  // Network change: the current transport dies and never reopens by itself.
  room.killCurrentTransport = () => { if (room.transport) room.transport.dead = true; };
  return room;
}

function createState(room, collection) {
  const transport = room.register(null);
  const state = {
    collection,
    transport,
    cancelled: false,
    error$: events(), active$: events(), canceled$: events(), transportStatus$: events(),
    peerStates$: {
      ...events(),
      getValue() {
        if (state.cancelled || transport.dead || transport.closed) return new Map();
        return new Map([['native', { remoteProtocol: { peerSession: { role: 'ctox_instance' } } }]]);
      },
    },
    peer: {
      connections: {
        get() {
          const open = !state.cancelled && !transport.dead && !transport.closed;
          return {
            channel: { readyState: open ? 'open' : 'connecting' },
            peer: { connectionState: open ? 'connected' : 'connecting' },
          };
        },
      },
    },
    async awaitInitialReplication() { return true; },
    async awaitInSync() { return true; },
    getTransportStatus() { return {}; },
    async cancel() {
      state.cancelled = true;
      room.unregister(state);
    },
  };
  // Replace the placeholder registration with the real state object.
  room.registrations.delete(null);
  room.registrations.add(state);
  // A bridge on a live transport reports active once replication runs
  // (diagnostics: connected), after startCollection recorded its start.
  setTimeout(() => { if (isOpen(state)) state.active$.emit(true); }, 50);
  return state;
}

function fixture() {
  const browserToken = 'rebringup-browser-token';
  const previousWindow = globalThis.window;
  const windowListeners = new Map();
  let reloads = 0;
  globalThis.window = {
    location: { href: 'https://business-os.test/', reload() { reloads += 1; } },
    addEventListener(name, listener) { windowListeners.set(name, listener); },
    removeEventListener(name) { windowListeners.delete(name); },
  };
  const room = createRoomModel();
  const statesByCollection = new Map();
  const collectionNames = ['business_commands', 'user_thread_states'];
  const raw = Object.fromEntries(collectionNames.map((name) => [name, {
    name,
    schema: { version: 0, primaryPath: 'id', async hash() { return `${name}-hash`; } },
    storageCollection: {}, observe() { return () => {}; },
  }]));
  const coordinator = {
    onRoleChange() { return () => {}; }, onDirty() { return () => {}; },
    async start() { return this.snapshot(); },
    isLeader() { return true; },
    snapshot() { return { isLeader: true, role: 'leader' }; },
    stop() {},
  };
  let lastDiagnostic = null;
  const runtime = createSyncRuntime({
    onDiagnostic(snapshot) { lastDiagnostic = snapshot; },
    db: {
      mode: 'rxdb', name: 'rebringup-test', raw,
      rxdb: {
        getMultiTabSyncCoordinator() { return coordinator; },
        getConnectionHandlerSimplePeer(options) { return options; },
        async replicateWebRTC({ collection }) {
          const state = createState(room, collection.name);
          statesByCollection.set(collection.name, state);
          return state;
        },
      },
    },
    config: {
      transport: 'webrtc', sync_room: 'ctox-business-os:rebringup-test',
      signaling_urls: ['wss://signal.test/room'],
      signaling_auth_version: 'ctox-role-bound-v1',
      signaling_browser_token: browserToken,
      signaling_browser_token_hash: createHash('sha256').update(browserToken).digest('hex'),
      signaling_native_token_hash: createHash('sha256').update('native-token').digest('hex'),
    },
    repairTimings: {
      repairCycleOpenTimeoutMs: 300,
      transportRebringupOpenTimeoutMs: 3_000,
      transportRebringupBackoffBaseMs: 50,
    },
  });
  return {
    runtime,
    room,
    statesByCollection,
    windowListeners,
    get reloads() { return reloads; },
    get diagnostic() { return lastDiagnostic; },
    async close() {
      await runtime.stop();
      globalThis.window = previousWindow;
    },
  };
}

async function waitFor(predicate, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  assert.fail(`timed out after ${timeoutMs}ms waiting for: ${label}`);
}

function isOpen(state) {
  return Boolean(state && !state.cancelled && !state.transport.dead && !state.transport.closed);
}

test('failed individual restart immediately re-brings up the whole shared transport', async () => {
  const f = fixture();
  try {
    await f.runtime.startCollection('business_commands');
    const lease = await f.runtime.leaseCollection('user_thread_states', 'open-window');
    assert.equal(f.room.transportsCreated, 1);
    assert.equal(f.room.registrations.size, 2);
    assert.ok(isOpen(f.statesByCollection.get('business_commands')));
    await waitFor(
      () => f.diagnostic?.collections?.business_commands?.connectionStatus === 'connected',
      2_000,
      'business_commands reported connected before the network switch',
    );

    // Network switch: the shared transport dies. Only user_thread_states
    // reports it; business_commands still holds a registration on the dead
    // room peer — exactly the incident shape.
    f.room.killCurrentTransport();
    const deadTransport = f.room.transport;
    f.statesByCollection.get('user_thread_states').error$.emit(
      Object.assign(new Error('ERR_CONNECTION_FAILURE'), { code: 'ERR_CONNECTION_FAILURE' }),
    );
    // The network is healthy again for any NEW transport.
    f.room.nextTransportDead = false;

    const startedAt = Date.now();
    await waitFor(
      () => ['business_commands', 'user_thread_states']
        .every((name) => isOpen(f.statesByCollection.get(name))),
      8_000,
      'every collection open on a fresh transport',
    );
    const recoveryMs = Date.now() - startedAt;

    assert.equal(deadTransport.closed, true, 'the dead shared transport must be closed');
    assert.ok(f.room.transportsCreated >= 2, 'a new transport (signaling + peer connection) must be created');
    for (const name of ['business_commands', 'user_thread_states']) {
      assert.notEqual(
        f.statesByCollection.get(name).transport, deadTransport,
        `${name} must run on the new transport`,
      );
    }
    assert.equal(f.reloads, 0, 'the runtime must never reload the page');
    await waitFor(() => f.diagnostic?.transportRebringup?.state === 'succeeded', 2_000, 'diagnostic emitted');
    assert.equal(f.diagnostic.transportRebringup.attempt, 1);
    assert.equal(f.diagnostic.transportRebringup.trigger, 'user_thread_states');
    const resources = f.runtime.resourceSnapshot();
    assert.deepEqual(resources.pinnedCollections, ['business_commands'], 'pins survive the re-bring-up');
    assert.equal(resources.leaseCounts.user_thread_states, 1, 'leases survive the re-bring-up');
    assert.ok(recoveryMs < 8_000, `recovered in ${recoveryMs}ms`);
    await lease.release();
  } finally {
    await f.close();
  }
});

test('concurrent re-bring-up requests share one in-flight run', async () => {
  const f = fixture();
  try {
    await f.runtime.startCollection('business_commands');
    const first = f.runtime.rebringUpTransport('business_commands');
    const second = f.runtime.rebringUpTransport('business_commands');
    assert.strictEqual(first, second, 'at most one full re-bring-up at a time');
    const stable = await first;
    assert.equal(stable.length, 1);
    assert.equal(f.room.transportsClosed, 1);
    assert.equal(f.room.transportsCreated, 2);
    assert.ok(isOpen(f.statesByCollection.get('business_commands')));
  } finally {
    await f.close();
  }
});

test('only the typed no-open-peer verdict escalates to a transport re-bring-up', () => {
  const verdict = Object.assign(new AggregateError([], 'none opened'), {
    code: 'peer_connect_timeout', retryable: true,
  });
  assert.equal(shouldEscalateToTransportRebringup(verdict), true);
  assert.equal(shouldEscalateToTransportRebringup({ code: 'peer_connect_timeout', retryable: false }), false);
  assert.equal(shouldEscalateToTransportRebringup({ code: 'ctox_schema_mismatch', retryable: false }), false);
  assert.equal(shouldEscalateToTransportRebringup(new Error('generic')), false);
  assert.equal(shouldEscalateToTransportRebringup(null), false);
});

test('re-bring-up backoff starts only at the third consecutive failure', () => {
  assert.equal(transportRebringupBackoffMs(0, 2_000, 60_000), 0);
  assert.equal(transportRebringupBackoffMs(1, 2_000, 60_000), 0);
  assert.equal(transportRebringupBackoffMs(2, 2_000, 60_000), 0);
  assert.equal(transportRebringupBackoffMs(3, 2_000, 60_000), 2_000);
  assert.equal(transportRebringupBackoffMs(4, 2_000, 60_000), 4_000);
  assert.equal(transportRebringupBackoffMs(5, 2_000, 60_000), 8_000);
  assert.equal(transportRebringupBackoffMs(50, 2_000, 60_000), 60_000);
});

test('background repair escalates within roughly 15 s (attempt + individual retry)', () => {
  assert.ok(REPAIR_CYCLE_OPEN_TIMEOUT_MS * 2 <= 15_000);
});
