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
];
const joined = JSON.stringify({
  type: 'joined',
  peers: [{ peerId: 'ctox-core-test', role: 'ctox_instance' }],
});

const peers = criticalCollections.map((collection) => createPeer(collection, `browser-${collection}`));
for (const peer of peers) peer.handleSignalingMessage(joined);

const optionalPeer = createPeer('desktop_windows', 'browser-optional');
optionalPeer.handleSignalingMessage(joined);
await delay(10);

const snapshot = optionalPeer.getTransportStatus().rtcConnectionPool;
assertEqual(snapshot.active, 5, 'critical collections must consume the active RTC pool first');
assertEqual(snapshot.queued, 1, 'optional collection must wait for critical DataChannels');
assertEqual(snapshot.criticalReady, false, 'critical pool must wait for datachannel-open, not slot allocation');
assertEqual(snapshot.queuedConnections?.[0]?.collection, 'desktop_windows', 'optional collection should be queued');
assertEqual(snapshot.activeConnections?.map((entry) => entry.collection).join(','), criticalCollections.join(','), 'active pool must contain only shell-critical collections');

for (const peer of [...peers, optionalPeer]) peer.close();
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
