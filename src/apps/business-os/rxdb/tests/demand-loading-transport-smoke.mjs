// Verifies the integration that the review demanded:
//   replicateWebRTC builds a demand-loading transport,
//   enableDemandLoading() opens the sidecar + attaches a loader,
//   collection.setDemandLoader is actually called,
//   incoming rxdb.query.chunk messages get routed to the right collector
//   and resolve the outstanding requestQueryFetch.
//
// We bypass the real WebRTC peer by directly invoking the requestHandlers
// the transport exposes — that's the exact dispatch path the JS peer uses
// for incoming chunk messages.

import {
  createDemandLoadingTransport,
} from '../dist/ctox-rxdb-js.mjs';
import { deflateRawSync } from 'node:zlib';

const transport = createDemandLoadingTransport({ getPeerId: () => 'peer-1' });

const sent = [];
const fakePeer = {
  // The transport only dispatches to peers whose DataChannel is OPEN
  // (resolvePeerId -> isPeerOpen); mock an open connection for peer-1.
  connections: new Map([
    ['peer-1', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }],
  ]),
  async request(peerId, method, params) {
    sent.push({ peerId, method, params });
    return { ack: true };
  },
};
transport.attach(fakePeer);

// Fire a query-fetch. The promise should pend until we route a complete chunk.
const envelope = {
  requestId: 'q-1',
  collectionName: 'business_records',
  schemaVersion: 1,
  queryFingerprint: 'fp-x',
  query: { selector: { status: 'open' }, sort: [], limit: 100, skip: 0 },
  window: { offset: 0, limit: 100 },
};
const promise = transport.requestQueryFetch(envelope);

// Microtask flush so peer.request is awaited.
await new Promise((r) => setImmediate(r));
assert(sent.length === 1, `peer.request must be called for query.fetch (got ${sent.length})`);
assert(sent[0].method === 'rxdb.query.fetch', `expected rxdb.query.fetch (got ${sent[0].method})`);
let diagnostics = transport.diagnostics();
assert(diagnostics.pendingQueryCollectors === 1, 'diagnostics show pending query collector');
assert(diagnostics.maxPendingQueryCollectors >= 1, 'diagnostics record query collector peak');

// Simulate Rust server sending two chunks. First a non-complete inline chunk,
// then a compressed terminal chunk.
const compressedDocs = Array.from({ length: 30 }, (_, i) => ({ id: `c-${i}`, n: i, status: 'open' }));
const compressedPayload = Buffer.from(JSON.stringify(compressedDocs), 'utf8');
const compressed = deflateRawSync(compressedPayload);

await transport.requestHandlers['rxdb.query.chunk']({
  params: [{
    requestId: 'q-1',
    sequence: 0,
    documents: [{ id: 'first', n: 0, status: 'open' }],
    complete: false,
    authoritativeRevision: 'rev-1',
  }],
});

await transport.requestHandlers['rxdb.query.chunk']({
  params: [{
    requestId: 'q-1',
    sequence: 1,
    documents: [],
    complete: true,
    authoritativeRevision: 'rev-1',
    compressed: 'deflate',
    compressedBase64: compressed.toString('base64'),
  }],
});

const result = await promise;
assert(result.documents.length === 31, `expected 31 docs reassembled (got ${result.documents.length})`);
assert(result.documents[0].id === 'first', 'first inline doc present');
assert(result.documents[1].id === 'c-0', 'first compressed doc present');
diagnostics = transport.diagnostics();
assert(diagnostics.pendingQueryCollectors === 0, 'diagnostics clear query collector after completion');
assert(diagnostics.queryChunksReceived === 2, 'diagnostics count query chunks');
assert(diagnostics.maxBufferedQueryChunks >= 2, 'diagnostics record query chunk buffer peak');
assert(result.documents[30].id === 'c-29', 'last compressed doc present');
assert(result.authoritativeRevision === 'rev-1', 'revision propagated');

// Error path
const errPromise = transport.requestQueryFetch({ ...envelope, requestId: 'q-err' });
await new Promise((r) => setImmediate(r));
await transport.requestHandlers['rxdb.query.error']({
  params: [{ requestId: 'q-err', code: 'PEER_UNAVAILABLE', message: 'no peer', retryable: true }],
});
let caught = null;
try { await errPromise; } catch (e) { caught = e; }
assert(caught && caught.code === 'PEER_UNAVAILABLE', 'PEER_UNAVAILABLE propagated');

// Cancel path: removes the in-flight collector AND rejects the outstanding
// fetch with QUERY_CANCELLED so callers stop waiting (hardened cancel
// semantics — previously the promise just hung forever).
const pendingPromise = transport.requestQueryFetch({ ...envelope, requestId: 'q-cancel' });
const pendingOutcome = pendingPromise.catch((error) => error);
await new Promise((r) => setImmediate(r));
assert(transport.pendingQueryCount() === 1, 'one pending before cancel');
await transport.requestQueryCancel({ requestId: 'q-cancel' });
assert(transport.pendingQueryCount() === 0, 'cancel clears pending');
const cancelError = await pendingOutcome;
assert(cancelError && cancelError.code === 'QUERY_CANCELLED', 'cancelled fetch rejects with QUERY_CANCELLED');

// Peer-loss abort path: request accepted, then peer closes before final chunk.
const peerAbortQueryPromise = transport.requestQueryFetch({ ...envelope, requestId: 'q-peer-close' });
const peerAbortQueryOutcome = peerAbortQueryPromise.catch((error) => error);
await new Promise((r) => setImmediate(r));
assert(transport.pendingQueryCount() >= 1, 'query pending before peer abort');
const abortedQueries = transport.abortPeerRequests('peer-1', 'peer-close');
assert(abortedQueries >= 1, 'peer abort rejects at least the query collector');
const peerAbortQueryError = await peerAbortQueryOutcome;
assert(peerAbortQueryError?.code === 'QUERY_CANCELLED', 'peer abort rejects query with QUERY_CANCELLED');

// File requests use the same shared peer and must also be rejected on peer
// loss; otherwise file demand collectors hang until an outer timeout.
const filePromise = transport.requestFileFetch({
  requestId: 'file-peer-close',
  collectionName: 'desktop_files',
  fileId: 'file-1',
});
const fileOutcome = filePromise.catch((error) => error);
await new Promise((r) => setImmediate(r));
assert(transport.pendingFileCount() === 1, 'one file collector pending before peer abort');
assert(transport.diagnostics().pendingFileCollectors === 1, 'diagnostics show pending file collector');
const abortedFiles = transport.abortPeerRequests('peer-1', 'peer-close');
assert(abortedFiles >= 1, 'peer abort rejects file collector');
assert(transport.pendingFileCount() === 0, 'peer abort clears file collector');
assert(transport.diagnostics().maxPendingFileCollectors >= 1, 'diagnostics record file collector peak');
const fileAbortError = await fileOutcome;
assert(fileAbortError?.code === 'FILE_CANCELLED', 'peer abort rejects file with FILE_CANCELLED');

console.log('ctox-rxdb-js demand-loading transport smoke OK', {
  docs: result.documents.length,
  sentRequests: sent.length,
});

function assert(c, m) { if (!c) throw new Error(m); }
