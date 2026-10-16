// Customer tenant 16.10.2026: 124 moved leads never left one browser.
//
// outbound_lead_generation_leads is demand-only (pull: null), so it never owns a
// pull checkpoint. Native research writes move the collection epoch all the
// time. On every reconnect the retained checkpoints were judged by the PULL rule
// (`remoteCheckpointStillCovers` needs a retained pull lwt), which a demand-only
// collection can never satisfy, so the PUSH checkpoint was dropped as well. The
// push then restarted from null and re-sent every still-pushable local lead
// oldest-first. Measured on the tenant: 130 research cancellations (~13 MB) were
// re-applied at 09:04-09:08 and again at 09:21-09:23 while the 124 newer moves
// queued behind them and the connection renegotiated 9 times in 15 minutes.
//
// The push position lives in the browser store. It must survive a reconnect
// while the native storage generation, collection and schema are unchanged,
// even though the remote epoch moved and demand eviction bumped the local
// eviction generation. A storage-generation change, a local head that went
// backwards, or a role/epoch change of the browser identity still drop it.

import {
  replicateWebRTC,
  replicationWebRtcTestInternals as internals,
} from '../src/replication-webrtc.mjs';

let failures = 0;
function check(name, condition) {
  if (condition) console.log(`ok   ${name}`);
  else { failures += 1; console.log(`FAIL ${name}`); }
}

const STORE = '0f9c2d1e-1111-4222-8333-944455556666';
let localStatus = { lwt: 1792140536000, eviction: 3 };
function localCheckpoint() {
  return {
    source: 'browser',
    state: 'advertised',
    collection: 'outbound_lead_generation_leads',
    schemaHash: 'hash-leads',
    latestLwt: localStatus.lwt,
    epoch: `browser:outbound_lead_generation_leads:${localStatus.lwt}:abcdef0123456789`,
    evictionGeneration: localStatus.eviction,
    localStoreGeneration: STORE,
  };
}

function protocol(epoch, { generation = 'gen-thesen' } = {}) {
  return {
    storageGeneration: generation,
    checkpoint: { epoch, latestLwt: 1792140000000 },
    peerSession: { sessionId: 'rxdb-rs-thesen', role: 'ctox_instance' },
    collection: { name: 'outbound_lead_generation_leads', schemaHash: 'hash-leads' },
    capabilities: ['ctox-checkpoint-generation-v2'],
  };
}

const state = await replicateWebRTC({
  collection: {
    name: 'outbound_lead_generation_leads',
    schema: { version: 0, primaryPath: 'id', hash: async () => 'hash-leads' },
    observe() { return { unsubscribe() {} }; },
    storageCollection: {
      replicationCheckpointStatus: async () => localCheckpoint(),
      getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
      bulkWrite: async () => ({}),
    },
  },
  topic: 'room-thesen-push-resume-424242',
  connectionHandlerCreator: {
    kind: 'ctox-native-webrtc',
    signalingServerUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
    config: {},
  },
  // Demand-only, exactly like the leads bridge in shared/sync.js.
  pull: null,
  push: { batchSize: 100 },
  retryTime: 60,
  ctox: { capabilityTokenProvider: async () => '' },
});
state.initialReplication?.catch?.(() => {});
state.pullFromRemotePeers = async () => {};
state.pushToRemotePeers = async () => {};
let currentProtocol = protocol('epoch-a');
state.remoteProtocolForPeer = () => currentProtocol;

async function connect(peerId, proto) {
  currentProtocol = proto;
  await state.runPeerReady(peerId, proto, false);
}
function drop(peerId) {
  state.peerStates$.next(new Map([[peerId, { peerId }]]));
  state.removePeer(peerId, 'peer-close');
}

const pushedUpTo = { lwt: 1792140536000, id: 'lead_zzzz' };
try {
  await connect('peer-1', protocol('epoch-a'));
  state.pushCheckpointsByPeer.set('peer-1', pushedUpTo);
  await state.persistCheckpointsForPeer('peer-1');
  drop('peer-1');
  check('push checkpoint retained on peer-close', state.retainedCheckpoints?.push?.lwt === pushedUpTo.lwt);

  // Native research writes moved the epoch; demand eviction ran; the user
  // wrote locally (head forward). Same daemon storage generation.
  localStatus = { lwt: 1792140740000, eviction: 7 };
  await connect('peer-2', protocol('epoch-b'));
  check('reconnect after native writes resumes the push position',
    state.pushCheckpointsByPeer.get('peer-2')?.lwt === pushedUpTo.lwt);
  check('pull readiness is still invalidated (no pull checkpoint reused)',
    !state.pullCheckpointsByPeer.has('peer-2'));
  drop('peer-2');

  // A second renegotiation keeps resuming (the field pattern: many reconnects).
  localStatus = { lwt: 1792140800000, eviction: 9 };
  await connect('peer-3', protocol('epoch-c'));
  check('repeated reconnects keep resuming the push position',
    state.pushCheckpointsByPeer.get('peer-3')?.lwt === pushedUpTo.lwt);
  drop('peer-3');

  // A different native storage generation is a different master: re-push.
  await connect('peer-4', protocol('epoch-d', { generation: 'gen-rebuilt' }));
  check('native storage generation change drops the push checkpoint',
    !state.pushCheckpointsByPeer.has('peer-4'));
  check('native storage generation change clears persistent checkpoints', state.retainedCheckpoints === null);
} finally {
  await state.cancel();
}

// Helper rules, including the local head going backwards.
{
  const { remotePushCheckpointStillCovers: remote, localPushCheckpointStillCovers: local } = internals;
  const key = (head, gen = 'g') => `${gen}|outbound_lead_generation_leads|hash-leads|${head}`;
  check('remote: moved epoch with same generation covers', remote({ validityKey: key('a') }, key('b')));
  check('remote: generation change does not cover', !remote({ validityKey: key('a') }, key('b', 'h')));
  check('remote: legacy keys keep exact equality', !remote({ validityKey: 'e|s|h' }, 'e2|s|h'));
  const localKey = (lwt, ev) => `browser:outbound_lead_generation_leads:${lwt}:abcdef|hash-leads|ev${ev}|store${STORE}`;
  check('local: eviction with head forward covers', local(localKey(10, 1), localKey(20, 4)));
  check('local: head moved backwards does not cover', !local(localKey(20, 1), localKey(10, 2)));
  check('local: different store does not cover',
    !local(localKey(10, 1), localKey(20, 1).replace(STORE, '0f9c2d1e-1111-4222-8333-000000000000')));
}

if (failures) {
  console.error(`demand-only push checkpoint resume smoke: ${failures} failure(s)`);
  process.exit(1);
}
console.log('demand-only push checkpoint resume smoke OK');
process.exit(0);
