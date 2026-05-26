// V1.5 demand-loading transport.
//
// Sits next to a CtoxWebRtcReplicationState and turns the bidirectional
// peer.request channel into a request/response demand-loading layer:
// the browser issues `rxdb.query.fetch` / `rxdb.file.fetch` requests, the
// server (Rust dispatcher) pushes correlated chunk-message frames, this
// module collects them, decodes them, and resolves the corresponding
// outstanding requestQueryFetch / requestFileFetch promise.

import { decodeChunk } from './chunk-decoder.mjs';
import { CTOX_QUERY_RPC } from './protocol-contract.generated.mjs';

const ACK_RESPONSE = Object.freeze({ ack: true });

/// Build the request-handler map that should be merged into
/// `createCtoxWebRtcNativePeer({ requestHandlers })`. The returned object
/// also exposes `requestQueryFetch`, `requestFileFetch` etc.
export function createDemandLoadingTransport({ getPeerId } = {}) {
  if (typeof getPeerId !== 'function') {
    throw new TypeError('createDemandLoadingTransport requires getPeerId');
  }

  const queryCollectors = new Map();   // requestId -> { chunks, resolve, reject, decoded }
  const fileCollectors = new Map();    // requestId -> { chunks, resolve, reject }

  function routeQueryChunk(chunk) {
    if (!chunk || !chunk.requestId) return;
    const slot = queryCollectors.get(chunk.requestId);
    if (!slot) return;
    slot.chunks.push(chunk);
    if (chunk.complete) {
      queryCollectors.delete(chunk.requestId);
      slot.resolve(slot.chunks);
    }
  }
  function routeQueryError(err) {
    if (!err || !err.requestId) return;
    const slot = queryCollectors.get(err.requestId);
    if (!slot) return;
    queryCollectors.delete(err.requestId);
    const e = new Error(`${err.code || 'QUERY_ERROR'}: ${err.message || ''}`);
    e.code = err.code;
    e.retryable = Boolean(err.retryable);
    slot.reject(e);
  }
  function routeFileChunk(chunk) {
    if (!chunk || !chunk.requestId) return;
    const slot = fileCollectors.get(chunk.requestId);
    if (!slot) return;
    slot.chunks.push(chunk);
    if (chunk.complete) {
      fileCollectors.delete(chunk.requestId);
      slot.resolve(slot.chunks);
    }
  }
  function routeFileError(err) {
    if (!err || !err.requestId) return;
    const slot = fileCollectors.get(err.requestId);
    if (!slot) return;
    fileCollectors.delete(err.requestId);
    const e = new Error(`${err.code || 'FILE_ERROR'}: ${err.message || ''}`);
    e.code = err.code;
    e.retryable = Boolean(err.retryable);
    slot.reject(e);
  }

  const requestHandlers = {
    'rxdb.query.chunk': async ({ params }) => { routeQueryChunk(params?.[0]); return ACK_RESPONSE; },
    'rxdb.query.error': async ({ params }) => { routeQueryError(params?.[0]); return ACK_RESPONSE; },
    'rxdb.file.chunk':  async ({ params }) => { routeFileChunk(params?.[0]); return ACK_RESPONSE; },
    'rxdb.file.error':  async ({ params }) => { routeFileError(params?.[0]); return ACK_RESPONSE; },
  };

  let peer = null;
  function attach(p) { peer = p; }

  async function requestQueryFetch(envelope) {
    if (!peer) throw new Error('demand transport has no peer attached');
    const peerId = getPeerId();
    if (!peerId) throw new Error('PEER_UNAVAILABLE');
    const requestId = envelope.requestId;
    const promise = new Promise((resolve, reject) => {
      queryCollectors.set(requestId, { chunks: [], resolve, reject });
    });
    try {
      await peer.request(peerId, CTOX_QUERY_RPC.fetch, [envelope]);
    } catch (err) {
      queryCollectors.delete(requestId);
      throw err;
    }
    const chunks = await promise;
    const documents = [];
    let authoritativeRevision = null;
    for (const c of chunks) {
      const decoded = await decodeChunk(c);
      for (const d of decoded) documents.push(d);
      if (c.authoritativeRevision) authoritativeRevision = c.authoritativeRevision;
    }
    return { documents, authoritativeRevision };
  }

  async function requestQueryCancel({ requestId }) {
    if (!peer || !requestId) return;
    const peerId = getPeerId();
    if (!peerId) return;
    try {
      await peer.request(peerId, CTOX_QUERY_RPC.cancel, [{ requestId, reason: 'client-abort' }], 2000);
    } catch {
      // best-effort
    }
    queryCollectors.delete(requestId);
  }

  async function requestFileFetch({ requestId, fileId, range, knownSequences, collectionName }) {
    if (!peer) throw new Error('demand transport has no peer attached');
    const peerId = getPeerId();
    if (!peerId) throw new Error('PEER_UNAVAILABLE');
    const promise = new Promise((resolve, reject) => {
      fileCollectors.set(requestId, { chunks: [], resolve, reject });
    });
    try {
      await peer.request(peerId, 'rxdb.file.fetch', [{
        requestId,
        collectionName,
        fileId,
        range: range ?? null,
        knownSequences: knownSequences ?? [],
      }]);
    } catch (err) {
      fileCollectors.delete(requestId);
      throw err;
    }
    const chunks = await promise;
    return chunks.map((c) => ({ sequence: c.sequence, bytesBase64: c.bytesBase64, hash: c.hash }));
  }

  function pendingQueryCount() { return queryCollectors.size; }
  function pendingFileCount() { return fileCollectors.size; }

  return {
    requestHandlers,
    attach,
    requestQueryFetch,
    requestQueryCancel,
    requestFileFetch,
    pendingQueryCount,
    pendingFileCount,
  };
}
