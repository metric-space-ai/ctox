// REGRESSION (gating catch-up, browser side): the native peer DROPS
// master-change relays for collections outside the reported active set, and
// browser pulls are purely event-driven. When a collection transitions
// inactive -> active the shared peer MUST trigger one catch-up pull for it
// (state.onMasterChange()) — otherwise every event that landed while the
// collection was inactive is lost until the next unrelated native write
// (rxdb-soak workspace-large-file-viewer-restart: a ctox.file.materialize
// landing while desktop_files was inactive never reached the browser).
//
// Drives the REAL SharedRoomPeer registry wiring with a stubbed registry and
// peer — no network, no RxDB instance.

import { replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';

const SharedRoomPeer = replicationWebRtcTestInternals.getSharedRoomPeerClass();

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const shared = new SharedRoomPeer({
  key: 'test-key',
  signalingUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
  room: 'room-test-123456',
  iceServers: [],
  expectedNativePeerId: 'native-1',
});

// Stub the singleton registry BEFORE ensurePeer subscribes to it.
let registryListener = null;
shared.activeRegistry = {
  onChange(listener) {
    registryListener = listener;
    return () => { registryListener = null; };
  },
  activeCollectionsList() {
    return [];
  },
};
// Run the REAL ensurePeer: it constructs a CtoxWebRtcNativePeer but never
// connects it (connect happens in start()), so the only side effects are
// listener registrations — including the registry subscription under test.
const sentFrames = [];
const realPeer = shared.ensurePeer();
assert(realPeer, 'ensurePeer returned the peer');
assert(typeof registryListener === 'function', 'shared peer subscribed to the active-collection registry');

// Register two collections with spy replication states.
const pulls = { desktop_files: 0, business_commands: 0 };
for (const name of Object.keys(pulls)) {
  shared.collections.set(name, {
    state: {
      onMasterChange() { pulls[name] += 1; },
    },
  });
}
// Capture outbound active-collections frames instead of touching a channel.
shared.activeRemotePeerId = 'native-1';
shared.peer.send = (peerId, frame) => { sentFrames.push({ peerId, frame }); };

// 1. First report: business_commands active -> catch-up pull for it only.
registryListener(['business_commands']);
assert(pulls.business_commands === 1, 'newly active collection got a catch-up pull');
assert(pulls.desktop_files === 0, 'inactive collection got no pull');
assert(sentFrames.length === 1, 'active set was sent to the native peer');

// 2. Unchanged set: no extra pull.
registryListener(['business_commands']);
assert(pulls.business_commands === 1, 'unchanged set does not re-pull');

// 3. desktop_files becomes active: exactly one catch-up pull for it.
registryListener(['business_commands', 'desktop_files']);
assert(pulls.desktop_files === 1, 'reactivated collection got its catch-up pull');
assert(pulls.business_commands === 1, 'already-active collection not re-pulled');

// 4. Deactivate + reactivate: pulls again (events may have been dropped).
registryListener(['business_commands']);
registryListener(['business_commands', 'desktop_files']);
assert(pulls.desktop_files === 2, 'inactive->active transition pulls again');

console.log('ctox-rxdb active-collections catch-up smoke OK');
process.exit(0);
