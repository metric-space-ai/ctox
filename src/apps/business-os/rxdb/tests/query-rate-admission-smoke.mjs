import assert from 'node:assert/strict';
import { createDemandLoadingTransport } from '../dist/ctox-rxdb-js.mjs';

// THESEN 14.10: a busy Outbound tab exhausted the native per-peer query rate
// bucket while the owner opened and saved a lead. Every send, including a
// retry, must pass the shared browser budget before reaching the native peer.
let transport;
const sentAt = [];
const peer = {
  connections: new Map([['native', {
    channel: { readyState: 'open' }, peer: { connectionState: 'connected' },
  }]]),
  async request(_peerId, method, params) {
    if (method !== 'rxdb.query.fetch') return { ack: true };
    const requestId = params[0].requestId;
    sentAt.push(Date.now());
    queueMicrotask(() => transport.requestHandlers['rxdb.query.chunk']({
      params: [{ requestId, sequence: 0, documents: [], complete: true }],
    }));
    return { ack: true };
  },
};
transport = createDemandLoadingTransport({ getPeerId: () => 'native' });
transport.attach(peer);

for (let i = 0; i < 33; i++) {
  await transport.requestQueryFetch({
    requestId: `rate-${i}`, collectionName: 'outbound_lead_generation_leads',
    schemaVersion: 1, queryFingerprint: `rate-${i}`,
    query: { selector: {}, sort: [], limit: 1, skip: 0 },
    window: { offset: 0, limit: 1 },
  });
}
assert.equal(sentAt.length, 33);
assert.ok(sentAt[24] - sentAt[0] >= 45,
  `the 25th native fetch must wait for browser rate capacity (${sentAt[24] - sentAt[0]}ms)`);
assert.ok(sentAt[32] - sentAt[0] >= 500,
  `steady demand traffic must remain below the native rate (${sentAt[32] - sentAt[0]}ms)`);
assert.equal(transport.diagnostics().pendingQueryCollectors, 0);
