// REGRESSION: runtime-installed Business OS apps register module collections
// after the shell-critical shared WebRTC room is already open. The shared peer
// must not reuse the old room handshake for a newly registered collection,
// because that handshake's collectionSchemas map did not include the module
// schema and produced a false schema-hash mismatch.

import { replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';

const SharedRoomPeer = replicationWebRtcTestInternals.getSharedRoomPeerClass();

const shared = new SharedRoomPeer({
  key: 'late-collection-test',
  signalingUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
  room: 'room-late-collection',
  iceServers: [],
  expectedNativePeerId: 'native-1',
});

shared.negotiated = {
  peerId: 'native-1',
  remoteProtocol: { marker: 'old-handshake' },
  queryFetchCapable: false,
};
shared.peerOpenQueue = Promise.resolve();
shared.isPeerOpen = () => true;
shared.openSharedPeerIds = () => ['native-1'];

let renegotiations = 0;
shared.negotiatePeer = async (peerId) => {
  renegotiations += 1;
  const negotiated = {
    peerId,
    remoteProtocol: { marker: 'renegotiated-with-late-collection' },
    queryFetchCapable: true,
  };
  shared.negotiated = negotiated;
  return negotiated;
};

shared.catchUpRegisteredCollection = async () => {
  await shared.ensureNegotiatedPeer();
};

shared.register('runtime_app_items', {
  collection: 'runtime_app_items',
  state: {},
});

const catchUp = shared.collectionCatchUps.get('runtime_app_items');
if (!catchUp) throw new Error('late collection catch-up was not scheduled');
await catchUp;

assert(renegotiations === 1, `late collection must renegotiate once, got ${renegotiations}`);
assert(
  shared.negotiated?.remoteProtocol?.marker === 'renegotiated-with-late-collection',
  'late collection did not replace the stale room handshake',
);

console.log('ctox-rxdb late-collection renegotiation smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
