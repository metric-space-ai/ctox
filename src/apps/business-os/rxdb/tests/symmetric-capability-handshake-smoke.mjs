import { replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';
import { CtoxWebRtcNativePeer } from '../src/webrtc-native.mjs';
import {
  clearHybridLogicalClockTimeAnchor,
  hybridLogicalClockStatus,
  setHybridLogicalClockTimeAnchor,
} from '../src/hybrid-logical-clock.mjs';
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

// The native response can be prompt even when symmetric authorization waits
// much longer. Anchor the clock to the captured response round trip, not the
// time at which waitForRequest finally permits the handshake to finish.
const delayedShared = new SharedRoomPeer({
  key: 'post-response-delay-test',
  signalingUrl: 'wss://signaling.invalid',
  room: 'room-post-response-delay',
  expectedNativePeerId: 'native-1',
});
delayedShared.representativeCollection = () => ({ collection: 'records' });
const originalDateNow = Date.now;
const responseTimeMs = 1_722_000_000_000;
let simulatedNowMs = responseTimeMs;
let authorizationWaited = false;
try {
  Date.now = () => simulatedNowMs;
  delayedShared.peer = {
    connections: new Map([['native-1', openConnection()]]),
    async protocolPayload() { return protocol; },
    async request() {
      return { ...protocol, peerSession: { role: 'ctox_instance' }, nativeTimeMs: responseTimeMs + 30_000 };
    },
    async waitForRequest() {
      simulatedNowMs += 16 * 60 * 1000;
      authorizationWaited = true;
    },
    send() { return true; },
  };
  assert((await delayedShared.negotiatePeer('native-1'))?.peerId === 'native-1',
    'the authorized peer must complete negotiation after the delayed token observation');
  const anchored = hybridLogicalClockStatus();
  assert(authorizationWaited, 'the post-response authorization wait must be exercised');
  assert(Math.abs(anchored.nativeClockOffsetMs - 30_000) < 1_000,
    'a 16-minute post-response wait must not become native clock skew');
  assert(Math.abs(anchored.nativeClockObservedAtMs - responseTimeMs) < 1_000,
    'the anchor must retain the protocol response midpoint, not the later authorization time');
} finally {
  Date.now = originalDateNow;
  clearHybridLogicalClockTimeAnchor(delayedShared.clockAnchorSource);
}

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

const retiredConnection = { remotePeerId: 'native-1' };
const replacementConnection = {
  remotePeerId: 'native-1',
  peer: { close() {} },
  auxChannels: new Map(),
};
peer.connections.set('native-1', replacementConnection);
const closeEvents = [];
peer.on('peer-close', (event) => closeEvents.push(event.detail));
setHybridLogicalClockTimeAnchor(observedAtMs + 30_000, observedAtMs, 'replacement-peer');
peer.removeConnection('native-1', 'old-peer-failed', null, {
  reconnect: false,
  expectedConnection: retiredConnection,
});
assert(peer.connections.get('native-1') === replacementConnection && closeEvents.length === 0,
  'a late close from a retired connection must not remove or announce the replacement');
assert(hybridLogicalClockStatus().nativeClockOffsetMs === 30_000,
  'a retired connection close must preserve the replacement clock anchor');
peer.removeConnection('native-1', 'test-cleanup', null, {
  reconnect: false,
  expectedConnection: replacementConnection,
});

const originalRtcPeerConnection = globalThis.RTCPeerConnection;
try {
  globalThis.RTCPeerConnection = class FakeRTCPeerConnection {
    connectionState = 'new';
    iceConnectionState = 'new';
    iceGatheringState = 'new';
    signalingState = 'stable';
    close() { this.connectionState = 'closed'; }
  };
  const generationPeer = new CtoxWebRtcNativePeer({
    signalingUrl: 'wss://signaling.invalid',
    room: 'room-stale-peer-close',
  });
  generationPeer.shouldInitiate = () => false;
  const oldConnection = generationPeer.createConnection('native-1');
  clearTimeout(oldConnection.handshakeTimer);
  generationPeer.connections.delete('native-1');
  const liveConnection = generationPeer.createConnection('native-1');
  clearTimeout(liveConnection.handshakeTimer);
  const emittedCloses = [];
  generationPeer.on('peer-close', (event) => emittedCloses.push(event.detail));
  oldConnection.peer.connectionState = 'failed';
  oldConnection.peer.onconnectionstatechange();
  assert(generationPeer.connections.get('native-1') === liveConnection && emittedCloses.length === 0,
    'a late failed-state callback from the retired RTC peer must leave its successor connected');
  generationPeer.removeConnection('native-1', 'test-cleanup', null, {
    reconnect: false,
    expectedConnection: liveConnection,
  });
} finally {
  globalThis.RTCPeerConnection = originalRtcPeerConnection;
}

const sendPeer = new CtoxWebRtcNativePeer({
  signalingUrl: 'wss://signaling.invalid',
  room: 'room-stale-send-buffer',
});
const queuedOldConnection = {
  remotePeerId: 'native-1',
  channel: { readyState: 'open', bufferedAmount: 0 },
};
const queuedNewConnection = { remotePeerId: 'native-1', channel: { readyState: 'open' } };
sendPeer.connections.set('native-1', queuedOldConnection);
const sendCloseEvents = [];
sendPeer.on('peer-close', (event) => sendCloseEvents.push(event.detail));
const originalWaitForSendBuffer = sendPeer.waitForSendBuffer.bind(sendPeer);
let enteredSendBuffer;
const sendBufferEntered = new Promise((resolve) => { enteredSendBuffer = resolve; });
let releaseSendBuffer;
sendPeer.waitForSendBuffer = () => {
  enteredSendBuffer();
  return new Promise((resolve) => { releaseSendBuffer = resolve; });
};
assert(sendPeer.enqueueSendFrame(queuedOldConnection, {
  priority: 'normal', inline: true, text: '{}', payload: {},
}), 'the old connection must start draining its queued frame');
await sendBufferEntered;
sendPeer.connections.set('native-1', queuedNewConnection);
setHybridLogicalClockTimeAnchor(observedAtMs + 30_000, observedAtMs, 'queued-replacement');
releaseSendBuffer();
await new Promise((resolve) => setImmediate(resolve));
assert(sendPeer.connections.get('native-1') === queuedNewConnection && sendCloseEvents.length === 0,
  'a retired send queue must not remove the replacement after its buffer wait settles');
assert(hybridLogicalClockStatus().nativeClockOffsetMs === 30_000,
  'a retired send queue must preserve the replacement clock anchor');
sendPeer.waitForSendBuffer = originalWaitForSendBuffer;

const stalledOldConnection = {
  remotePeerId: 'native-2',
  channel: {
    bufferedAmount: Number.MAX_SAFE_INTEGER,
    bufferedAmountLowThreshold: 0,
    addEventListener() {},
    removeEventListener() {},
  },
};
const stalledNewConnection = { remotePeerId: 'native-2' };
sendPeer.connections.set('native-2', stalledOldConnection);
const originalSetTimeout = globalThis.setTimeout;
let fireStallTimeout;
try {
  globalThis.setTimeout = (callback) => {
    fireStallTimeout = callback;
    return 0;
  };
  const stalledWait = sendPeer.waitForSendBuffer(stalledOldConnection.channel, stalledOldConnection);
  sendPeer.connections.set('native-2', stalledNewConnection);
  fireStallTimeout();
  await stalledWait;
  assert(sendPeer.connections.get('native-2') === stalledNewConnection && sendCloseEvents.length === 0,
    'a retired send-buffer timeout must not remove or announce the replacement');
} finally {
  globalThis.setTimeout = originalSetTimeout;
}

const requestPeer = new CtoxWebRtcNativePeer({
  signalingUrl: 'wss://signaling.invalid',
  room: 'room-stale-request-timeout',
});
const requestOldConnection = { remotePeerId: 'native-3' };
const requestNewConnection = { remotePeerId: 'native-3' };
requestPeer.connections.set('native-3', requestOldConnection);
requestPeer.send = () => true;
const requestCloseEvents = [];
requestPeer.on('peer-close', (event) => requestCloseEvents.push(event.detail));
let fireRequestTimeout;
try {
  globalThis.setTimeout = (callback) => {
    fireRequestTimeout = callback;
    return 0;
  };
  const oldRequest = requestPeer.request('native-3', 'ctoxProtocol', [], 1);
  requestPeer.connections.set('native-3', requestNewConnection);
  fireRequestTimeout();
  await assertRejectsMessage(oldRequest, /Timed out waiting for WebRTC response ctoxProtocol/,
    'the old request must time out');
  assert(requestPeer.connections.get('native-3') === requestNewConnection
    && requestCloseEvents.length === 0,
  'a retired protocol request timeout must not recycle the replacement connection');
} finally {
  globalThis.setTimeout = originalSetTimeout;
}

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

async function assertRejectsMessage(promise, pattern, message) {
  try {
    await promise;
  } catch (error) {
    assert(pattern.test(String(error?.message || error)), `${message}: unexpected error`);
    return;
  }
  throw new Error(`${message}: promise resolved`);
}
