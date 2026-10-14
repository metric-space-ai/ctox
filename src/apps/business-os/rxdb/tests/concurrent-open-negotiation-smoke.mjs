// Collection catch-ups can all await the same RTCDataChannel opening. Once it
// opens, they must join one protocol negotiation rather than issue one framed
// ctoxProtocol request per collection over the serial ACK window.
import assert from 'node:assert/strict';
import { replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';

const SharedRoomPeer = replicationWebRtcTestInternals.getSharedRoomPeerClass();
const shared = new SharedRoomPeer({
  key: 'concurrent-open-negotiation-test',
  signalingUrl: 'wss://signaling.invalid/',
  room: 'room-concurrent-open',
  iceServers: [],
  expectedNativePeerId: 'native-1',
});

let releaseOpen;
const open = new Promise((resolve) => { releaseOpen = resolve; });
let channelOpen = false;
let negotiations = 0;
shared.openSharedPeerIds = () => [];
shared.waitForOpenSharedPeerId = async () => {
  await open;
  return 'native-1';
};
shared.isPeerOpen = (peerId) => channelOpen && peerId === 'native-1';
shared.negotiatePeer = async (peerId) => {
  negotiations += 1;
  // Let every waiter reach the negotiation boundary before the first one
  // settles. The real peer serializes these responses behind frame ACKs.
  await new Promise((resolve) => setImmediate(resolve));
  const result = { peerId, remoteProtocol: {}, queryFetchCapable: true };
  shared.negotiated = result;
  return result;
};

const waiters = Array.from({ length: 24 }, () => shared.ensureNegotiatedPeer());
await Promise.resolve();
channelOpen = true;
releaseOpen();
const results = await Promise.all(waiters);

assert.equal(negotiations, 1, 'one opened channel must produce one protocol negotiation');
assert.ok(results.every((result) => result === results[0]), 'every waiter receives the same authority');
assert.equal(await shared.ensureNegotiatedPeer(), results[0], 'later callers reuse the accepted authority');
assert.equal(negotiations, 1);

console.log('ctox-rxdb concurrent-open protocol negotiation smoke OK');
