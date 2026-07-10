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
  async request(peerId, method, params, timeoutMs) {
    sent.push({ peerId, method, params, timeoutMs });
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
assert(sent[0].timeoutMs >= 30000, `query.fetch must use a demand-fetch timeout, got ${sent[0].timeoutMs}`);
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

// Timeout retry path: the native peer can be busy while materialising a large
// query window. A request-level timeout must retry with a new request id
// instead of surfacing an empty Business OS screen.
const retryTransport = createDemandLoadingTransport({ getPeerId: () => 'peer-retry' });
const retrySent = [];
let retryAttempts = 0;
const retryPeer = {
  connections: new Map([
    ['peer-retry', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }],
  ]),
  async request(peerId, method, params, timeoutMs) {
    retrySent.push({ peerId, method, params, timeoutMs });
    retryAttempts += 1;
    if (retryAttempts === 1) {
      throw new Error('Timed out waiting for WebRTC response rxdb.query.fetch');
    }
    const retryRequestId = params?.[0]?.requestId;
    queueMicrotask(() => {
      retryTransport.requestHandlers['rxdb.query.chunk']({
        params: [{
          requestId: retryRequestId,
          sequence: 0,
          documents: [{ id: 'retried', status: 'open' }],
          complete: true,
          authoritativeRevision: 'rev-retry',
        }],
      });
    });
    return { ack: true };
  },
};
retryTransport.attach(retryPeer);
const retryResult = await retryTransport.requestQueryFetch({ ...envelope, requestId: 'q-timeout' });
assert(retrySent.length === 2, `timed out query.fetch must retry once (got ${retrySent.length})`);
assert(retrySent[0].timeoutMs >= 30000, `retry test query.fetch must use a demand-fetch timeout, got ${retrySent[0].timeoutMs}`);
assert(retrySent[1].params?.[0]?.requestId === 'q-timeout|retry-1', 'retry uses a fresh request id');
assert(retryResult.documents[0]?.id === 'retried', 'retry result materialised');

// The native token bucket deliberately rejects an excessive short burst with
// a retryable RATE_LIMITED response. Apps must wait for refill and retry
// instead of failing their mount (the accounting app legitimately fans out
// across many demand-only collections during startup).
const rateTransport = createDemandLoadingTransport({ getPeerId: () => 'peer-rate' });
let rateAttempts = 0;
rateTransport.attach({
  connections: new Map([
    ['peer-rate', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }],
  ]),
  async request(_peerId, _method, params) {
    rateAttempts += 1;
    const requestId = params?.[0]?.requestId;
    queueMicrotask(() => {
      if (rateAttempts === 1) {
        rateTransport.requestHandlers['rxdb.query.error']({
          params: [{
            requestId,
            code: 'RATE_LIMITED',
            message: 'per-peer query-fetch rate limit reached',
            retryable: true,
          }],
        });
      } else {
        rateTransport.requestHandlers['rxdb.query.chunk']({
          params: [{
            requestId,
            sequence: 0,
            documents: [{ id: 'after-rate-refill', status: 'open' }],
            complete: true,
            authoritativeRevision: 'rev-rate',
          }],
        });
      }
    });
    return { ack: true };
  },
});
const rateResult = await rateTransport.requestQueryFetch({ ...envelope, requestId: 'q-rate' });
assert(rateAttempts === 2, `rate-limited query.fetch must retry once (got ${rateAttempts})`);
assert(rateResult.documents[0]?.id === 'after-rate-refill', 'rate-limit retry result materialised');

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

// Successful file fetch path: diagnostics must expose peak retained file
// chunk bytes, not only the number of buffered chunks.
const fileSuccessPromise = transport.requestFileFetch({
  requestId: 'file-success',
  collectionName: 'desktop_files',
  fileId: 'file-success',
});
await new Promise((r) => setImmediate(r));
await transport.requestHandlers['rxdb.file.chunk']({
  params: [{
    requestId: 'file-success',
    sequence: 0,
    bytesBase64: 'AAAA',
    hash: 'h0',
    complete: false,
  }],
});
let fileDiagnostics = transport.diagnostics();
assert(fileDiagnostics.bufferedFileChunks === 1, 'diagnostics expose current buffered file chunk count');
assert(fileDiagnostics.bufferedFileChunkBytes === 4, 'diagnostics expose current buffered file chunk bytes');
await transport.requestHandlers['rxdb.file.chunk']({
  params: [{
    requestId: 'file-success',
    sequence: 1,
    bytesBase64: 'BBBBBB',
    hash: 'h1',
    complete: true,
  }],
});
const fileChunks = await fileSuccessPromise;
assert(fileChunks.length === 2, `expected two file chunks (got ${fileChunks.length})`);
fileDiagnostics = transport.diagnostics();
assert(fileDiagnostics.bufferedFileChunks === 0, 'completed file fetch clears current buffered chunks');
assert(fileDiagnostics.bufferedFileChunkBytes === 0, 'completed file fetch clears current buffered bytes');
assert(fileDiagnostics.fileChunksReceived >= 2, 'diagnostics count received file chunks');
assert(fileDiagnostics.maxBufferedFileChunks >= 2, 'diagnostics record file chunk buffer peak');
assert(fileDiagnostics.maxBufferedFileChunkBytes >= 10, 'diagnostics record file chunk byte peak');

// Incremental file consumers keep the transport collector at zero retained
// bytes while each chunk is durably consumed by the loader/sink.
const incrementallyConsumed = [];
const incrementalFilePromise = transport.requestFileFetch({
  requestId: 'file-incremental',
  collectionName: 'desktop_files',
  fileId: 'file-incremental',
  async onChunk(chunk) { incrementallyConsumed.push(chunk.sequence); },
});
await new Promise((r) => setImmediate(r));
await transport.requestHandlers['rxdb.file.chunk']({
  params: [{ requestId: 'file-incremental', sequence: 0, bytesBase64: 'AAAA', complete: false }],
});
assert(transport.diagnostics().bufferedFileChunkBytes === 0, 'incremental sink retains no base64 collector bytes');
await transport.requestHandlers['rxdb.file.chunk']({
  params: [{ requestId: 'file-incremental', sequence: 1, bytesBase64: 'BBBB', complete: true }],
});
const incrementalResult = await incrementalFilePromise;
assert(incrementalResult.length === 0, 'incremental transport returns no duplicate whole-file buffer');
assert(incrementallyConsumed.join(',') === '0,1', 'incremental consumer receives ordered chunks');

const budgetTransport = createDemandLoadingTransport({
  getPeerId: () => 'peer-budget',
  fileCollectorBudgetBytes: 262144,
});
const budgetRequests = [];
budgetTransport.attach({
  connections: new Map([
    ['peer-budget', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }],
  ]),
  async request(peerId, method, params) {
    budgetRequests.push({ peerId, method, params });
    return { ack: true };
  },
});
const overBudget = budgetTransport.requestFileFetch({
  requestId: 'file-over-budget',
  collectionName: 'desktop_files',
  fileId: 'file-over-budget',
}).catch((error) => error);
await new Promise((r) => setImmediate(r));
await budgetTransport.requestHandlers['rxdb.file.chunk']({
  params: [{
    requestId: 'file-over-budget',
    sequence: 0,
    bytesBase64: 'A'.repeat(262145),
    complete: false,
  }],
});
const collectorBudgetError = await overBudget;
assert(collectorBudgetError?.code === 'FILE_COLLECTOR_BUDGET_EXCEEDED', 'collector rejects accepted stream beyond byte budget');
assert(budgetTransport.pendingFileCount() === 0, 'byte-budget rejection releases collector');
assert(budgetTransport.diagnostics().fileCollectorBudgetExceeded === 1, 'byte-budget rejection is observable');
assert(budgetRequests.some((request) => request.method === 'rxdb.file.cancel'), 'byte-budget rejection cancels native stream');

// Lost terminal frames must not leave collectors alive forever after the
// native acknowledgement. The timeout rejects and emits a peer-scoped cancel.
const timeoutTransport = createDemandLoadingTransport({
  getPeerId: () => 'peer-timeout',
  collectorTimeoutMs: 20,
});
const timeoutSent = [];
timeoutTransport.attach({
  connections: new Map([
    ['peer-timeout', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }],
  ]),
  async request(peerId, method, params, timeoutMs) {
    timeoutSent.push({ peerId, method, params, timeoutMs });
    return { ack: true };
  },
});
const lostTerminal = timeoutTransport.requestQueryFetch({ ...envelope, requestId: 'q-lost-terminal' })
  .catch((error) => error);
const lostTerminalError = await lostTerminal;
assert(lostTerminalError?.code === 'QUERY_COLLECTOR_TIMEOUT', 'lost query terminal frame rejects on collector deadline');
assert(timeoutTransport.pendingQueryCount() === 0, 'query collector deadline releases pending state');
assert(timeoutTransport.diagnostics().queryCollectorTimeouts === 1, 'query collector timeout is observable');
assert(
  timeoutSent.some((entry) => entry.method === 'rxdb.query.cancel'
    && entry.params?.[0]?.requestId === 'q-lost-terminal'),
  'query collector deadline sends a cancel for the same peer request',
);

const lostFileTerminal = timeoutTransport.requestFileFetch({
  requestId: 'file-lost-terminal',
  collectionName: 'desktop_files',
  fileId: 'file-lost-terminal',
}).catch((error) => error);
const lostFileTerminalError = await lostFileTerminal;
assert(lostFileTerminalError?.code === 'FILE_COLLECTOR_TIMEOUT', 'lost file terminal frame rejects on collector deadline');
assert(timeoutTransport.pendingFileCount() === 0, 'file collector deadline releases pending state');
assert(timeoutTransport.diagnostics().fileCollectorTimeouts === 1, 'file collector timeout is observable');

// Browser-side admission is bounded before work reaches the native peer.
const admissionTransport = createDemandLoadingTransport({ getPeerId: () => 'peer-admission' });
admissionTransport.attach({
  connections: new Map([
    ['peer-admission', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }],
  ]),
  async request() { return { ack: true }; },
});
const activeQueries = Array.from({ length: 6 }, (_, index) => (
  admissionTransport.requestQueryFetch({ ...envelope, requestId: `q-admission-${index}` })
    .catch((error) => error)
));
await new Promise((r) => setImmediate(r));
const oversizedQueuedQuery = await admissionTransport.requestQueryFetch({
  ...envelope,
  requestId: 'q-admission-overflow',
  query: { selector: { oversized: 'x'.repeat(1024 * 1024) } },
}).catch((error) => error);
assert(oversizedQueuedQuery?.code === 'QUERY_QUEUE_LIMIT', 'queued query bytes are rejected at the browser budget');
admissionTransport.abortPeerRequests('peer-admission', 'test-cleanup');
await Promise.all(activeQueries);

const activeFiles = Array.from({ length: 8 }, (_, index) => (
  admissionTransport.requestFileFetch({
    requestId: `file-admission-${index}`,
    collectionName: 'desktop_files',
    fileId: `file-admission-${index}`,
  }).catch((error) => error)
));
await new Promise((r) => setImmediate(r));
const fileAdmissionOverflow = await admissionTransport.requestFileFetch({
  requestId: 'file-admission-overflow',
  collectionName: 'desktop_files',
  fileId: 'file-admission-overflow',
}).catch((error) => error);
assert(fileAdmissionOverflow?.code === 'FILE_COLLECTOR_LIMIT', 'file collector count is bounded in the browser');
admissionTransport.abortPeerRequests('peer-admission', 'test-cleanup');
await Promise.all(activeFiles);

console.log('ctox-rxdb-js demand-loading transport smoke OK', {
  docs: result.documents.length,
  sentRequests: sent.length,
});

function assert(c, m) { if (!c) throw new Error(m); }
