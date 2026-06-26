// REGRESSION: transport-status snapshots must not be rebuilt/emitted once per
// WebRTC frame. Large file transfers generate many frame counters quickly; the
// peer should coalesce those updates while keeping getTransportStatus() live.

import { createCtoxWebRtcNativePeer } from '../src/webrtc-native.mjs';

const peer = createCtoxWebRtcNativePeer({
  signalingUrl: 'ws://127.0.0.1:9/signaling',
  room: 'ctox-rxdb-transport-status-throttle',
});

const emissions = [];
peer.on('transport-status', (event) => {
  emissions.push(event.detail || event);
});

for (let index = 0; index < 100; index += 1) {
  peer.recordTransportStatus({ sentFrames: index + 1 });
}

assert(emissions.length === 1, `expected one immediate coalesced emission, got ${emissions.length}`);
assert(peer.getTransportStatus().sentFrames === 100, 'live status should expose the latest counter immediately');

await delay(320);

assert(
  emissions.length === 2,
  `expected one delayed coalesced emission after burst, got ${emissions.length}`,
);
assert(emissions.at(-1).sentFrames === 100, 'delayed emission should contain the latest counter');

for (let index = 0; index < 20; index += 1) {
  peer.recordMessageMeta(`peer-${index}`, {
    id: `message-${index}`,
    method: 'rxdb.pull',
    collection: 'desktop_file_chunks',
  });
}

assert(
  emissions.length === 2,
  `message metadata burst should also be throttled, got ${emissions.length} emissions`,
);

await delay(320);

assert(
  emissions.length === 3,
  `expected metadata burst to coalesce into one delayed emission, got ${emissions.length}`,
);
assert(
  emissions.at(-1).recentMessages === undefined,
  'default transport-status emissions must stay skinny and omit recent messages',
);
assert(
  peer.getTransportStatus().rtcConnections === undefined,
  'default getTransportStatus() must not build RTC connection snapshots',
);
const diagnostics = peer.getTransportStatus({ includeDiagnostics: true });
assert(diagnostics.recentMessages.length === 20, 'explicit diagnostics snapshot should retain recent messages');
assert(Array.isArray(diagnostics.rtcConnections), 'explicit diagnostics snapshot should include RTC connection snapshots');

peer.close();

console.log('ctox-rxdb transport status throttle smoke OK');

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
