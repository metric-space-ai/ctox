import assert from 'node:assert/strict';
import { createDemandLoadingTransport } from '../dist/ctox-rxdb-js.mjs';
import { CLIENT_QUERY_QUEUE_LIMIT } from '../src/demand-loading-transport.mjs';

const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const flush = () => new Promise(resolve => setImmediate(resolve));
const envelope = requestId => ({
  requestId, collectionName: 'business_records', schemaVersion: 1,
  queryFingerprint: requestId, query: { selector: {}, limit: 1 },
  window: { offset: 0, limit: 1 },
});
const metrics = [];
function actor(name, initiallyReady = false, autoComplete = true) {
  let authorized = initiallyReady;
  const sent = [];
  const transport = createDemandLoadingTransport({ getPeerId: () => authorized ? name : '' });
  const complete = requestId => transport.requestHandlers['rxdb.query.chunk']({
    params: [{ requestId, sequence: 0, documents: [], complete: true }],
  });
  transport.attach({
    connections: new Map([[name, { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }]]),
    async request(peerId, method, [request]) {
      if (method === 'rxdb.query.fetch') {
        sent.push(request.requestId);
        metrics.push(transport.diagnostics().activeQueryStreams);
        if (autoComplete) queueMicrotask(() => complete(request.requestId));
      }
      return { ack: true };
    },
  });
  return { transport, sent, complete,
    authorize: () => { authorized = true; }, deauthorize: () => { authorized = false; } };
}
async function until(predicate) {
  const deadline = Date.now() + 2000;
  while (!predicate()) {
    assert(Date.now() < deadline, 'admitted query did not progress');
    await delay(5);
  }
}

// Six unopened collection transports used to consume every active stream slot.
const waiting = actor('waiting');
const ready = actor('ready', true);
const pending = Array.from({ length: 6 }, (_, n) => waiting.transport.requestQueryFetch(envelope('waiting-' + n)));
await flush();
assert.equal(waiting.transport.diagnostics().activeQueryStreams, 0);
const begin = performance.now();
await Promise.race([
  ready.transport.requestQueryFetch(envelope('ready-query')),
  delay(250).then(() => { throw new Error('ready peer was blocked by pending handshakes'); }),
]);
const readyMs = performance.now() - begin;
assert.equal(waiting.sent.length, 0, 'authorization must precede any native request');
waiting.authorize();
await Promise.all(pending);
await flush();
assert.equal(waiting.sent.length, 6);
assert.equal(ready.transport.diagnostics().activeQueryStreams, 0);

// Cancellation is scoped to the owning transport even for identical IDs.
const first = actor('cancel-owner');
const second = actor('other-owner');
const cancelled = first.transport.requestQueryFetch(envelope('shared-id')).catch(error => error);
const surviving = second.transport.requestQueryFetch(envelope('shared-id'));
await flush();
await first.transport.requestQueryCancel({ requestId: 'shared-id' });
assert.equal((await cancelled).code, 'QUERY_CANCELLED');
assert.equal(second.transport.diagnostics().queuedQueryRequests, 1);
first.authorize();
second.authorize();
await surviving;
await delay(110);
assert.deepEqual(first.sent, []);
assert.deepEqual(second.sent, ['shared-id']);

// Handshake waiters retain the existing admission bound. A ready peer can
// still use an active slot when the waiting queue reaches its count limit.
const full = actor('full');
const fullPending = Array.from({ length: CLIENT_QUERY_QUEUE_LIMIT }, (_, n) =>
  full.transport.requestQueryFetch(envelope('full-' + n)).catch(error => error));
const overflow = await full.transport.requestQueryFetch(envelope('overflow')).catch(error => error);
assert.equal(overflow.code, 'QUERY_QUEUE_LIMIT');
await ready.transport.requestQueryFetch(envelope('ready-through-full-waiting-queue'));
assert.equal(full.sent.length, 0);
assert.equal(full.transport.abortPeerRequests('full', 'test-close'), CLIENT_QUERY_QUEUE_LIMIT);
assert((await Promise.all(fullPending)).every(error => error.code === 'QUERY_CANCELLED'));
full.authorize();
await delay(110);
assert.deepEqual(full.sent, [], 'aborted waiters cannot revive after authentication');
assert.equal(full.transport.diagnostics().queuedQueryRequests, 0);

// Waiting entries cannot block already-ready entries after active slots free.
const busy = actor('busy', true, false);
const notReady = actor('not-ready');
const active = Array.from({ length: 6 }, (_, n) => busy.transport.requestQueryFetch(envelope('active-' + n)));
await until(() => busy.sent.length === 6);
const behind = notReady.transport.requestQueryFetch(envelope('still-waiting')).catch(error => error);
const lostAuthorization = actor('lost-authorization', true);
const lostPending = lostAuthorization.transport.requestQueryFetch(envelope('was-ready'));
lostAuthorization.deauthorize();
const eligible = busy.transport.requestQueryFetch(envelope('eligible'));
await busy.complete('active-0');
await until(() => busy.sent.includes('eligible'));
assert.equal(notReady.sent.length, 0);
assert.equal(lostAuthorization.sent.length, 0, 'queued admission must recheck current authorization');
lostAuthorization.authorize();
assert(metrics.every(active => active <= 6), 'queue handoff must never over-admit native streams');
for (const id of [...busy.sent]) await busy.complete(id);
await Promise.all([...active, eligible, lostPending]);
notReady.transport.abortPeerRequests('not-ready', 'test-close');
await behind;
await delay(110);
assert.equal(busy.transport.diagnostics().activeQueryStreams, 0);
assert.equal(busy.transport.diagnostics().queuedQueryRequests, 0);
console.log('query_admission_readiness=' + JSON.stringify({
  componentFixture: true, realWebRtc: false, readyResponseMs: readyMs,
  peakActiveStreams: Math.max(...metrics),
  pendingHandshakeIsolation: true, ownerScopedCancellation: true,
  queueBoundPreserved: true, noLateDispatchAfterAbort: true,
}));
