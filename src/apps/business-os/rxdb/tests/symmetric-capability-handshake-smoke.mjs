import { replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';
import { CtoxWebRtcNativePeer } from '../src/webrtc-native.mjs';
import { hybridLogicalClockStatus, setHybridLogicalClockTimeAnchor } from '../src/hybrid-logical-clock.mjs';
import { CTOX_REQUIRED_PROTOCOL_CAPABILITIES, CTOX_RXDB_PROTOCOL } from '../src/protocol-contract.generated.mjs';
import { webcrypto } from 'node:crypto';

// Node 18 does not expose WebCrypto globally unless started with an opt-in
// flag. Production browsers do, and newer Node releases do; install the native
// Node implementation here so the server-side release gate exercises the same
// random-id path instead of failing before the handshake assertion.
globalThis.crypto ??= webcrypto;

const SharedRoomPeer = replicationWebRtcTestInternals.getSharedRoomPeerClass();
const shared = new SharedRoomPeer({
  key: 'symmetric-capability-test',
  signalingUrl: 'wss://signaling.invalid',
  room: 'room-symmetric-capability',
  iceServers: [],
  expectedNativePeerId: 'native-1',
});

let observedTimeoutMs = 0;
shared.peer = {
  async waitForRequest(peerId, method, timeoutMs) {
    assert(peerId === 'native-1', 'remote master readiness uses the negotiated peer');
    assert(method === 'token', 'remote master readiness waits for native token request');
    observedTimeoutMs = timeoutMs;
  },
};
await shared.awaitRemoteMasterReady('native-1');
assert(
  observedTimeoutMs >= 10_000,
  `symmetric capability handshake must tolerate a busy native peer, got ${observedTimeoutMs}ms`,
);

const handshakeError = new Error('native symmetric handshake missing');
shared.peer = {
  async waitForRequest() {
    throw handshakeError;
  },
};
await assertRejects(
  shared.awaitRemoteMasterReady('native-1'),
  handshakeError,
  'missing native authorization handshake must fail closed',
);

const observedAtMs = Date.now();
setHybridLogicalClockTimeAnchor(observedAtMs + 60_000, observedAtMs, 'trusted-peer');
const protocol = {
  protocol: CTOX_RXDB_PROTOCOL,
  capabilities: [...CTOX_REQUIRED_PROTOCOL_CAPABILITIES],
  collection: { name: 'records', schemaVersion: 1, schemaHash: 'test-hash' },
};
const openConnection = () => ({
  channel: { readyState: 'open' },
  peer: { connectionState: 'connected' },
});
shared.representativeCollection = () => ({ collection: 'records' });
shared.peer = {
  connections: new Map([['native-1', openConnection()]]),
  async protocolPayload() { return protocol; },
  async request() {
    return { ...protocol, peerSession: { role: 'ctox_instance' }, nativeTimeMs: observedAtMs - 600_000 };
  },
  async waitForRequest() { throw handshakeError; },
};
await assertRejects(
  shared.negotiatePeer('native-1'),
  handshakeError,
  'a failed native authorization handshake must reject room negotiation',
);
assert(hybridLogicalClockStatus().nativeClockOffsetMs === 60_000,
  'an unauthorized peer must not replace the existing clock anchor');

shared.peer.waitForRequest = async () => {
  shared.peer.connections.set('native-1', openConnection());
};
assert(await shared.negotiatePeer('native-1') === null,
  'a connection replaced during authorization must not complete negotiation');
assert(hybridLogicalClockStatus().nativeClockOffsetMs === 60_000,
  'a replaced peer generation must not change the clock anchor');

const peer = new CtoxWebRtcNativePeer({
  signalingUrl: 'wss://signaling.invalid',
  room: 'room-observed-request-reset',
});
peer.observedRequests.set('native-1|token', Date.now());
assert(peer.hasObservedRequest('native-1', 'token'), 'precondition: token request is observed');
peer.removeConnection('native-1', 'reconnect', null, { reconnect: false });
assert(
  !peer.hasObservedRequest('native-1', 'token'),
  'a reconnect must not reuse an earlier connection token observation',
);

console.log('ctox-rxdb symmetric capability handshake smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function assertRejects(promise, expected, message) {
  try {
    await promise;
  } catch (error) {
    assert(error === expected, `${message}: rejected with an unexpected error`);
    return;
  }
  throw new Error(`${message}: promise resolved`);
}
