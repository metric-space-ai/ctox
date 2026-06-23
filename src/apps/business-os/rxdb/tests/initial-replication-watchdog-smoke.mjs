import { __ctoxSyncTestHooks } from '../../shared/sync.js';

const { initialReplicationProgressSignature } = __ctoxSyncTestHooks;

const base = signature({
  receivedFrames: 10,
  lwt: 1,
  pullInProgress: true,
});

assert(
  signature({ receivedFrames: 11, lwt: 1, pullInProgress: true }) !== base,
  'received WebRTC frames must count as initial replication progress',
);
assert(
  signature({ receivedFrames: 10, lwt: 2, pullInProgress: true }) !== base,
  'pull checkpoint movement must count as initial replication progress',
);
assert(
  signature({ receivedFrames: 10, lwt: 1, pendingRequests: 1, pullInProgress: true }) !== base,
  'pending WebRTC requests must count as initial replication progress',
);
assert(
  signature({ receivedFrames: 10, lwt: 1, pullInProgress: false }) !== base,
  'pull/push activity changes must count as initial replication progress',
);

console.log('initial replication watchdog smoke OK');

function signature({
  receivedFrames = 0,
  pendingRequests = 0,
  lwt = 0,
  pullInProgress = false,
  pushInProgress = false,
} = {}) {
  const peerId = 'native-peer';
  const state = {
    pullInProgress,
    pushInProgress,
    peerStates$: {
      getValue: () => new Map([
        [peerId, { remoteProtocol: { peerSession: { role: 'ctox_instance' } } }],
      ]),
    },
    peer: {
      connections: new Map([
        [peerId, {
          channel: { readyState: 'open' },
          peer: { connectionState: 'connected' },
        }],
      ]),
    },
    pullCheckpointsByPeer: new Map([[peerId, { id: 'checkpoint', lwt }]]),
    pushCheckpointsByPeer: new Map(),
    getTransportStatus: () => ({
      receivedFrames,
      pendingRequests,
      pullInProgress,
      pushInProgress,
    }),
  };
  return initialReplicationProgressSignature(state);
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
