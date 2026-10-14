import assert from 'node:assert/strict';
import { createDemandLoadingTransport } from '../src/demand-loading-transport.mjs';

// A real peer admits 64 initial fetches and 16 more per second. Shell startup
// can ask for dozens of windows before a user opens a lead. The browser must
// reserve headroom and admit the foreground read before queued shell windows.
const started = [];
let transport;
const peer = {
  connections: new Map([['native', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }]]),
  async request(_peerId, method, params) {
    assert.equal(method, 'rxdb.query.fetch');
    const request = params[0];
    started.push({ collection: request.collectionName, at: Date.now() });
    queueMicrotask(() => transport.requestHandlers['rxdb.query.chunk']({
      params: [{ requestId: request.requestId, sequence: 0, documents: [], complete: true }],
    }));
    return { ack: true };
  },
};
transport = createDemandLoadingTransport({ getPeerId: () => 'native' });
transport.attach(peer);
const fetch = (i, collectionName) => transport.requestQueryFetch({
  requestId: `rate-${i}`, collectionName, schemaVersion: 1,
  queryFingerprint: `rate-${i}`, query: { selector: {}, sort: [], limit: 1, skip: 0 },
  window: { offset: 0, limit: 1 },
});
const background = Array.from({ length: 59 }, (_, i) => fetch(i, 'business_commands'));
const deadline = Date.now() + 1000;
while (started.length < 48 && Date.now() < deadline) {
  await new Promise(resolve => setTimeout(resolve, 5));
}
assert.equal(started.length, 48, 'startup must use the bounded initial burst');
const foreground = fetch(59, 'outbound_lead_generation_leads');
await Promise.all([...background, foreground]);
assert.equal(started.length, 60);
assert.equal(started[48].collection, 'outbound_lead_generation_leads',
  'the first refilled rate token must serve the foreground read');
const firstAt = started[0].at;
assert.ok(started.filter(entry => entry.at - firstAt < 900).length <= 48,
  'browser must not send more than 48 fetches in its initial peer burst');
console.log('query rate admission smoke passed');
