// Pins the Phase-3 multiplex admission contract + the shell-critical set.
//
// HISTORY: before Phase 3 every collection opened its own RTCPeerConnection
// and a per-collection admission gate held optional collections back until
// the shell-critical DataChannels were open. Phase 3 multiplexes EVERY
// collection of a sync room over ONE RTCPeerConnection, so the gate is
// intentionally retired (rtcPeerConnectionPriority always returns 0). This
// test used to pin the retired gate and was red for weeks — which trained
// agents to ignore failing tests. It now pins the CURRENT contract:
//
//   1. SHELL_CRITICAL_COLLECTIONS is the single source of truth app.js
//      derives from; its membership changing silently is a drift bug.
//   2. Multiplexed admission: connections are granted immediately (no
//      per-collection queueing), with the documented pool headroom.
//
// If you (the agent reading this) change either contract, change it in
// src/webrtc-native.mjs FIRST, on purpose, and update this pin in the same
// commit — never by deleting assertions to make the suite pass.

globalThis.window = {};
globalThis.document = {};
globalThis.RTCPeerConnection = class FakeRTCPeerConnection {
  constructor() {
    this.connectionState = 'new';
    this.iceConnectionState = 'new';
    this.iceGatheringState = 'new';
    this.signalingState = 'stable';
    this.localDescription = null;
    this.remoteDescription = null;
  }

  createDataChannel() {
    return {
      readyState: 'connecting',
      send() {},
      close() {},
      addEventListener() {},
      removeEventListener() {},
    };
  }

  async createOffer() {
    return { type: 'offer', sdp: 'fake-offer' };
  }

  async setLocalDescription(description) {
    this.localDescription = description;
    this.signalingState = 'have-local-offer';
  }

  close() {
    this.connectionState = 'closed';
    this.signalingState = 'closed';
  }
};

const { createCtoxWebRtcNativePeer, SHELL_CRITICAL_COLLECTIONS } = await import('../dist/ctox-rxdb-js.mjs');

// --- 1. shell-critical set drift guard ---------------------------------------
const EXPECTED_SHELL_CRITICAL = [
  'ctox_runtime_settings',
  'business_module_catalog',
  'business_commands',
  'ctox_queue_tasks',
  'browser_sessions',
  'browser_tabs',
  'browser_frames',
  'browser_input_events',
];
const actualCritical = [...SHELL_CRITICAL_COLLECTIONS].sort();
const expectedCritical = [...EXPECTED_SHELL_CRITICAL].sort();
assertEqual(
  JSON.stringify(actualCritical),
  JSON.stringify(expectedCritical),
  'SHELL_CRITICAL_COLLECTIONS membership changed — update this pin (and app.js consumers) deliberately',
);

// --- 2. multiplexed admission: all rooms get RTC slots immediately -----------
const joined = JSON.stringify({
  type: 'joined',
  peers: [{ peerId: 'ctox-core-test', role: 'ctox_instance' }],
});

const ROOM_COUNT = 18;
const peers = Array.from({ length: ROOM_COUNT }, (_, index) => createPeer(`room-${index}`, `browser-${index}`));
for (const peer of peers) peer.handleSignalingMessage(joined);
await delay(10);

const snapshot = peers[0].getTransportStatus().rtcConnectionPool;
assertEqual(snapshot.maxActive, 64, 'browser RTC pool headroom (documented cap)');
assertEqual(snapshot.active, ROOM_COUNT, 'multiplexed rooms must all be granted RTC slots immediately');
assertEqual(snapshot.queued, 0, 'the retired per-collection admission gate must not queue connections');

for (const peer of peers) peer.close();
console.log('ctox-rxdb-js rtc critical pool smoke OK (multiplex contract)');

function createPeer(room, clientId) {
  return createCtoxWebRtcNativePeer({
    signalingUrl: 'wss://signaling.invalid',
    room: `ctox-business-os:instance:secret:${room}`,
    clientId,
  });
}

function assertEqual(actual, expected, message) {
  if (actual !== expected) {
    throw new Error(`${message}: expected ${expected}, got ${actual}`);
  }
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
