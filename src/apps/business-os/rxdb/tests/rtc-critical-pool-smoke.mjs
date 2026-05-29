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

const { createCtoxWebRtcNativePeer } = await import('../dist/ctox-rxdb-js.mjs');

const criticalCollections = [
  'ctox_runtime_settings',
  'business_module_catalog',
  'business_commands',
  'ctox_queue_tasks',
  'desktop_files',
  'desktop_file_chunks',
  'browser_sessions',
  'browser_tabs',
  'browser_frames',
  'browser_input_events',
];
const joined = JSON.stringify({
  type: 'joined',
  peers: [{ peerId: 'ctox-core-test', role: 'ctox_instance' }],
});

const peers = criticalCollections.map((collection) => createPeer(collection, `browser-${collection}`));
for (const peer of peers) peer.handleSignalingMessage(joined);

const optionalPeers = Array.from({ length: 8 }, (_, index) => createPeer(`desktop_windows_${index}`, `browser-optional-${index}`));
for (const peer of optionalPeers) peer.handleSignalingMessage(joined);
await delay(10);

const snapshot = optionalPeers[0].getTransportStatus().rtcConnectionPool;
assertEqual(snapshot.maxActive, 64, 'browser RTC pool must leave headroom after shell-critical startup');
assertEqual(snapshot.active, criticalCollections.length, 'only critical browser collections should receive RTC slots before readiness');
assertEqual(snapshot.queued, optionalPeers.length, 'optional collections must wait for critical DataChannels');
assertEqual(snapshot.criticalReady, false, 'critical pool must wait for datachannel-open, not slot allocation');
assertEqual(snapshot.queuedConnections?.[0]?.collection, 'desktop_windows_0', 'optional collections should be queued in request order');
assertEqual(
  snapshot.activeConnections?.filter((entry) => criticalCollections.includes(entry.collection)).length,
  criticalCollections.length,
  'active pool must contain every shell-critical and browser-critical collection'
);

for (const peer of [...peers, ...optionalPeers]) peer.close();
console.log('ctox-rxdb-js rtc critical pool smoke OK');

function createPeer(collection, clientId) {
  return createCtoxWebRtcNativePeer({
    signalingUrl: 'wss://signaling.invalid',
    room: `ctox-business-os:instance:secret:${collection}`,
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
