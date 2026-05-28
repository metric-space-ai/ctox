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
const SERVER_QUERY_STREAM_LIMIT = Math.max(1, Number(CTOX_QUERY_RPC.maxInFlightStreams) || 4);
const CLIENT_QUERY_STREAM_LIMIT = Math.max(1, Math.min(6, SERVER_QUERY_STREAM_LIMIT - 1 || 1));
const QUERY_STREAM_LIMIT_RETRY_MS = 160;
const QUERY_STREAM_LIMIT_RETRIES = 6;
const QUERY_PEER_RETRY_MS = 250;
const QUERY_PEER_RETRIES = 24;
const GLOBAL_QUERY_STREAM_STATE_KEY = Symbol.for('ctox.rxdb.query-stream-state.v1');

/// Build the request-handler map that should be merged into
/// `createCtoxWebRtcNativePeer({ requestHandlers })`. The returned object
/// also exposes `requestQueryFetch`, `requestFileFetch` etc.
export function createDemandLoadingTransport({ getPeerId } = {}) {
  if (typeof getPeerId !== 'function') {
    throw new TypeError('createDemandLoadingTransport requires getPeerId');
  }

  const queryCollectors = new Map();   // requestId -> { chunks, resolve, reject, decoded }
  const fileCollectors = new Map();    // requestId -> { chunks, resolve, reject }
  const queryStreamState = getGlobalQueryStreamState();

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
    return withQueryStreamSlot(() => requestQueryFetchWithRetry(envelope));
  }

  function withQueryStreamSlot(fn) {
    return new Promise((resolve, reject) => {
      const run = () => {
        queryStreamState.active += 1;
        Promise.resolve()
          .then(fn)
          .then(resolve, reject)
          .finally(() => {
            queryStreamState.active = Math.max(0, queryStreamState.active - 1);
            const next = queryStreamState.queue.shift();
            if (next) queueMicrotask(next);
          });
      };
      if (queryStreamState.active < CLIENT_QUERY_STREAM_LIMIT) run();
      else queryStreamState.queue.push(run);
    });
  }

  async function requestQueryFetchWithRetry(envelope) {
    const baseRequestId = envelope?.requestId;
    let attempt = 0;
    for (;;) {
      const requestId = attempt === 0 ? baseRequestId : `${baseRequestId}|retry-${attempt}`;
      try {
        return await requestQueryFetchOnce({ ...envelope, requestId });
      } catch (error) {
        const peerUnavailable = isRetryableQueryPeerUnavailable(error);
        const retryLimit = peerUnavailable ? QUERY_PEER_RETRIES : QUERY_STREAM_LIMIT_RETRIES;
        if (!isRetryableQueryFetch(error) || attempt >= retryLimit) {
          throw error;
        }
        attempt += 1;
        await delay((peerUnavailable ? QUERY_PEER_RETRY_MS : QUERY_STREAM_LIMIT_RETRY_MS) * attempt);
      }
    }
  }

  async function requestQueryFetchOnce(envelope) {
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

  function isRetryableQueryStreamLimit(error) {
    const code = String(error?.code || '');
    const message = String(error?.message || '');
    return Boolean(error?.retryable) && (code === 'STREAM_LIMIT_EXCEEDED' || message.includes('STREAM_LIMIT_EXCEEDED'));
  }

  function isRetryableQueryFetch(error) {
    return isRetryableQueryStreamLimit(error)
      || isRetryableQueryPeerUnavailable(error);
  }

  function isRetryableQueryPeerUnavailable(error) {
    const message = String(error?.message || '');
    return message === 'PEER_UNAVAILABLE'
      || /WebRTC peer .* is not open/.test(message);
  }

  function delay(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
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

  function pendingQueryCount() { return queryCollectors.size + queryStreamState.queue.length; }
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

function getGlobalQueryStreamState() {
  if (!globalThis[GLOBAL_QUERY_STREAM_STATE_KEY]) {
    globalThis[GLOBAL_QUERY_STREAM_STATE_KEY] = { active: 0, queue: [] };
  }
  return globalThis[GLOBAL_QUERY_STREAM_STATE_KEY];
}
