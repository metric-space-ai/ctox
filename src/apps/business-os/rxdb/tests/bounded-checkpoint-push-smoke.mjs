// REGRESSION: a checkpoint push sends byte-bounded masterWrite slices.
//
// `pushToPeer` read up to `push.batchSize` changed documents and sent them in
// ONE masterWrite. Researched outbound leads average 73 KiB (up to 237 KiB);
// with a batch size of 100 a single write was several MB, timed out on every
// retry (RC_PUSH), and 157 lead writes never left one browser, so its initial
// replication and the post-upgrade readiness ack never completed (thesen
// 09.10.2026). The direct push path already bounded its batches to 2 MiB.
//
// Contract pinned here:
//   1. every masterWrite of a checkpoint push stays within 2 MiB;
//   2. every changed document is written exactly once;
//   3. the push checkpoint advances past the whole read only after all slices.

import { replicateWebRTC } from '../src/replication-webrtc.mjs';

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const MAX_BYTES = 2 * 1024 * 1024;
const bytesOf = (value) => new TextEncoder().encode(JSON.stringify(value)).byteLength;

const collection = {
  name: 'outbound_lead_generation_leads',
  schema: { version: 0, primaryPath: 'id', hash: async () => 'hash-leads' },
  observe() { return { unsubscribe() {} }; },
  storageCollection: {
    conflictStrategy: 'lww',
    replicationCheckpointStatus: async () => ({ epoch: 'e1', state: 'ready' }),
    getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
    bulkWrite: async () => ({}),
  },
};

const state = await replicateWebRTC({
  collection,
  topic: 'room-outbound-leads-abcdef',
  connectionHandlerCreator: {
    kind: 'ctox-native-webrtc',
    signalingServerUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
    config: {},
  },
  pull: { batchSize: 100 },
  push: { batchSize: 100 },
  retryTime: 60,
});
state.initialReplication?.catch?.(() => {});

const evidence = 'x'.repeat(600 * 1024);
const leads = Array.from({ length: 6 }, (_, index) => ({
  id: `lead_${index}`,
  research_status: 'needs_review',
  payload: { evidence },
  updated_at_ms: 100 + index,
}));
let reads = 0;
state.collection.storageCollection.getChangedDocumentsSince = async () => {
  reads += 1;
  if (reads === 1) {
    return { documents: leads, checkpoint: { lwt: 105, id: 'lead_5' }, scanned: 6, scanLimitReached: false };
  }
  return { documents: [], checkpoint: { lwt: 105, id: 'lead_5' }, scanned: 0, scanLimitReached: false };
};
const writes = [];
state.shared.peer = {
  request: async (_peerId, method, params) => {
    assert(method === 'masterWrite', `expected masterWrite, got ${method}`);
    writes.push(params[0]);
    return [];
  },
};

await state.pushToPeer('p1');

assert(writes.length >= 2, `a 3.6 MB read must be split, got ${writes.length} masterWrite call(s)`);
for (const rows of writes) {
  const size = bytesOf(rows.map((row) => row.newDocumentState));
  assert(size <= MAX_BYTES, `masterWrite of ${size} bytes exceeds the 2 MiB bound`);
}
const sent = writes.flat().map((row) => row.newDocumentState.id).sort();
assert(
  JSON.stringify(sent) === JSON.stringify(leads.map((lead) => lead.id).sort()),
  `every changed lead is written exactly once, got ${sent.join(',')}`,
);
assert(state.pushCheckpointsByPeer.get('p1')?.id === 'lead_5', 'the checkpoint advanced past the whole read');
await state.cancel();

console.log(`bounded-checkpoint-push-smoke ok (${writes.length} slices)`);
// The replication state keeps reconnect timers to the unreachable test
// signaling URL; exit explicitly like the other replicateWebRTC smokes.
process.exit(0);
