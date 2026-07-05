// Presence (ctox-presence-v1) smoke: the ephemeral "who is editing what"
// path. Pins three contracts:
//   1. Registry semantics — per-owner local sets union into one wire set,
//      debounced; the refresh timer exists ONLY while local entries exist
//      (idle stays idle); remote aggregates replace wholesale.
//   2. SharedRoomPeer wiring — local changes go out as `rxdb.presence.update`
//      ONLY when the handshake advertised ctox-presence-v1 (a pre-presence
//      native peer must never see the unknown method); the `presence$` server
//      push lands in the registry; room teardown clears remote hints.
//   3. Presence is transport-only: no collection, no persistence, no HTTP.
//
// Drives the REAL SharedRoomPeer + CtoxWebRtcNativePeer frame routing with
// stubbed registries — no network, no RxDB instance.

import { createPresenceRegistry } from '../src/presence.mjs';
import {
  remoteSupportsPresence,
  replicationWebRtcTestInternals,
} from '../src/replication-webrtc.mjs';
import { CTOX_PRESENCE_CAPABILITY, CTOX_PRESENCE_RPC } from '../src/protocol-contract.generated.mjs';

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};
const sleep = (ms) => new Promise((resolveFn) => setTimeout(resolveFn, ms));

// ---------------------------------------------------------------------------
// 1. Registry semantics.
// ---------------------------------------------------------------------------
{
  const registry = createPresenceRegistry({ refreshMs: 40 });
  const localCalls = [];
  registry.onLocalChange((entries, meta) => localCalls.push({ entries, meta }));
  assert(localCalls.length === 1 && localCalls[0].entries.length === 0,
    'local listener fires immediately with the (empty) current set');

  registry.setLocal('module-a', [{ collection: 'c', recordId: 'r1', actorId: 'a' }]);
  registry.setLocal('module-b', [{ collection: 'c', recordId: 'r2', actorId: 'b' }, 'not-an-object']);
  await sleep(150);
  const union = registry.localEntries();
  assert(union.length === 2, `owners union to one wire set (got ${union.length})`);
  const changeCalls = localCalls.filter((c) => !c.meta.refresh);
  assert(changeCalls.length === 2, 'burst of owner updates debounced into one change notification');

  // Refresh ticks re-stamp the native TTL clock while entries exist...
  const refreshBefore = localCalls.filter((c) => c.meta.refresh).length;
  await sleep(100);
  const refreshAfter = localCalls.filter((c) => c.meta.refresh).length;
  assert(refreshAfter > refreshBefore, 'refresh tick fired while local entries exist');

  // ...and STOP when the set empties: idle must stay idle.
  registry.clearLocal('module-a');
  registry.clearLocal('module-b');
  await sleep(150);
  assert(registry.localEntries().length === 0, 'cleared owners empty the wire set');
  assert(registry.refreshTimer == null, 'refresh timer torn down once no local entries exist');

  const remoteCalls = [];
  registry.onRemoteChange((entries) => remoteCalls.push(entries));
  assert(remoteCalls.length === 1, 'remote listener fires immediately');
  registry.applyRemote([{ actorId: 'x' }, 42, null]);
  assert(remoteCalls.length === 2 && remoteCalls[1].length === 1,
    'remote aggregate replaces wholesale and drops non-object entries');
}

// ---------------------------------------------------------------------------
// 2. Capability gate helper.
// ---------------------------------------------------------------------------
{
  assert(!remoteSupportsPresence(null), 'null protocol: no presence');
  assert(!remoteSupportsPresence({ capabilities: ['ctox-rxdb-native-v1'] }),
    'pre-presence native peer: no presence');
  assert(remoteSupportsPresence({ capabilities: [CTOX_PRESENCE_CAPABILITY] }),
    'advertised capability: presence on');
}

// ---------------------------------------------------------------------------
// 3. SharedRoomPeer wiring (real ensurePeer + real frame routing).
// ---------------------------------------------------------------------------
{
  const SharedRoomPeer = replicationWebRtcTestInternals.getSharedRoomPeerClass();
  const shared = new SharedRoomPeer({
    key: 'test-key',
    signalingUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
    room: 'room-test-123456',
    iceServers: [],
    expectedNativePeerId: 'native-1',
  });

  // Stub BOTH singleton registries BEFORE ensurePeer subscribes to them.
  shared.activeRegistry = {
    onChange() { return () => {}; },
    activeCollectionsList() { return []; },
  };
  let presenceListener = null;
  const applied = [];
  shared.presenceRegistry = {
    onLocalChange(listener) {
      presenceListener = listener;
      return () => { presenceListener = null; };
    },
    localEntries() { return []; },
    applyRemote(entries) { applied.push(entries); },
  };

  const realPeer = shared.ensurePeer();
  assert(realPeer, 'ensurePeer returned the peer');
  assert(typeof presenceListener === 'function', 'shared peer subscribed to the presence registry');

  const sentFrames = [];
  shared.activeRemotePeerId = 'native-1';
  shared.peer.send = (peerId, frame) => { sentFrames.push({ peerId, frame }); };

  // Capability gate: WITHOUT ctox-presence-v1 nothing goes on the wire.
  shared.presenceCapable = false;
  presenceListener([{ actorId: 'a' }], { refresh: false });
  assert(sentFrames.length === 0, 'no presence frame toward a pre-presence native peer');

  // With the capability: local changes go out as rxdb.presence.update.
  shared.presenceCapable = true;
  const entry = { collection: 'customer_accounts', recordId: 'r-1', actorId: 'a' };
  presenceListener([entry], { refresh: false });
  assert(sentFrames.length === 1, 'presence frame sent once capable');
  assert(sentFrames[0].frame.method === CTOX_PRESENCE_RPC.update, 'frame uses the contract method');
  assert(Array.isArray(sentFrames[0].frame.params?.[0])
    && sentFrames[0].frame.params[0][0].recordId === 'r-1',
    'frame params carry the entry list');

  // Inbound `presence$` push routes through the REAL frame handler into the
  // registry — and must NOT be treated as a pending-response or request.
  await shared.peer.handleDataChannelFrame('native-1', {
    id: CTOX_PRESENCE_RPC.streamId,
    result: { entries: [{ actorId: 'other', recordId: 'r-9' }] },
    error: null,
  });
  assert(applied.length === 1 && applied[0][0].actorId === 'other',
    'presence$ push landed in the registry');

  // Room teardown clears remote hints and the capability flag.
  shared.refCount = 1;
  shared.unregister('whatever');
  assert(applied.length === 2 && applied[1].length === 0, 'teardown cleared remote presence');
  assert(shared.presenceCapable === false, 'teardown reset the capability gate');
}

console.log('ctox-rxdb presence smoke OK');
process.exit(0);
