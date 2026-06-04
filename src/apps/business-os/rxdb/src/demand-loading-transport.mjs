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
const CANCELLED_QUERY_REQUEST_LIMIT = 256;

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
  const cancelledQueryRequests = new Map();

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
    return withQueryStreamSlot(envelope?.requestId, () => requestQueryFetchWithRetry(envelope));
  }

  function withQueryStreamSlot(requestId, fn) {
    return new Promise((resolve, reject) => {
      const run = () => {
        queryStreamState.active += 1;
        Promise.resolve()
          .then(fn)
          .then(resolve, reject)
          .finally(() => {
            queryStreamState.active = Math.max(0, queryStreamState.active - 1);
            const next = queryStreamState.queue.shift();
            if (next) queueMicrotask(typeof next === 'function' ? next : next.run);
          });
      };
      if (queryStreamState.active < CLIENT_QUERY_STREAM_LIMIT) run();
      else queryStreamState.queue.push({ requestId: String(requestId || ''), run, reject });
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
    const requestId = envelope?.requestId;
    const cancelReason = consumeQueryCancelReason(requestId);
    if (cancelReason) throw createQueryCancelError(cancelReason);
    if (!peer) throw new Error('demand transport has no peer attached');
    const peerId = resolvePeerId();
    if (!peerId) throw new Error('PEER_UNAVAILABLE');
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

  async function requestQueryCancel({ requestId, reason = 'client-abort' }) {
    if (!requestId) return;
    const matchingRequestIds = matchingQueryRequestIds(requestId);
    const queuedRequestIds = rejectQueuedQueryRequests(requestId, reason);
    if (!matchingRequestIds.length && !queuedRequestIds.length) {
      markQueryCancelled(requestId, reason);
    }
    const error = createQueryCancelError(reason);
    for (const activeRequestId of matchingRequestIds) {
      rejectQueryCollector(activeRequestId, error);
    }
    const cancelRequestIds = matchingRequestIds.length
      ? matchingRequestIds
      : queuedRequestIds.length
        ? []
        : [requestId];
    const peerId = peer ? resolvePeerId() : '';
    if (peer && peerId) {
      for (const activeRequestId of cancelRequestIds) {
        try {
          await peer.request(peerId, CTOX_QUERY_RPC.cancel, [{ requestId: activeRequestId, reason }], 2000);
        } catch {
          // best-effort
        }
      }
    }
  }

  async function requestFileFetch({ requestId, fileId, range, knownSequences, collectionName }) {
    if (!peer) throw new Error('demand transport has no peer attached');
    const peerId = resolvePeerId();
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

  function matchingQueryRequestIds(requestId) {
    const raw = String(requestId || '');
    if (!raw) return [];
    const ids = [];
    if (queryCollectors.has(raw)) ids.push(raw);
    const prefix = `${raw}|`;
    for (const id of queryCollectors.keys()) {
      if (id !== raw && id.startsWith(prefix)) ids.push(id);
    }
    return ids;
  }

  function rejectQueryCollector(requestId, error) {
    const slot = queryCollectors.get(requestId);
    if (!slot) return false;
    queryCollectors.delete(requestId);
    slot.reject(error);
    return true;
  }

  function rejectQueuedQueryRequests(requestId, reason) {
    const raw = String(requestId || '');
    if (!raw) return [];
    const prefix = `${raw}|`;
    const remaining = [];
    const rejectedIds = [];
    const error = createQueryCancelError(reason);
    for (const entry of queryStreamState.queue) {
      const queuedRequestId = queuedQueryRequestId(entry);
      if (queuedRequestId && (queuedRequestId === raw || queuedRequestId.startsWith(prefix))) {
        rejectedIds.push(queuedRequestId);
        entry.reject(error);
      } else {
        remaining.push(entry);
      }
    }
    if (rejectedIds.length) {
      queryStreamState.queue.splice(0, queryStreamState.queue.length, ...remaining);
    }
    return rejectedIds;
  }

  function queuedQueryRequestId(entry) {
    if (!entry || typeof entry === 'function') return '';
    return String(entry.requestId || '');
  }

  function markQueryCancelled(requestId, reason) {
    const raw = String(requestId || '');
    if (!raw) return;
    cancelledQueryRequests.set(raw, reason || 'client-abort');
    while (cancelledQueryRequests.size > CANCELLED_QUERY_REQUEST_LIMIT) {
      const oldest = cancelledQueryRequests.keys().next().value;
      cancelledQueryRequests.delete(oldest);
    }
  }

  function consumeQueryCancelReason(requestId) {
    const raw = String(requestId || '');
    if (!raw) return '';
    if (cancelledQueryRequests.has(raw)) {
      const reason = cancelledQueryRequests.get(raw);
      cancelledQueryRequests.delete(raw);
      return reason;
    }
    for (const [cancelledRequestId, reason] of cancelledQueryRequests) {
      if (raw.startsWith(`${cancelledRequestId}|`)) {
        cancelledQueryRequests.delete(cancelledRequestId);
        return reason;
      }
    }
    return '';
  }

  function createQueryCancelError(reason) {
    const error = new Error(`QUERY_CANCELLED: ${reason || 'client-abort'}`);
    error.code = 'QUERY_CANCELLED';
    error.retryable = false;
    return error;
  }

  function resolvePeerId() {
    const configured = getPeerId();
    if (configured) return configured;
    return firstOpenPeerId();
  }

  function firstOpenPeerId() {
    const entries = peer?.connections?.entries?.();
    if (!entries) return '';
    for (const [peerId, connection] of entries) {
      const channelState = connection?.channel?.readyState || connection?.channelReadyState || '';
      const peerState = connection?.peer?.connectionState
        || connection?.peerConnectionState
        || connection?.connectionState
        || '';
      if (channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(String(peerState))) {
        return peerId;
      }
    }
    return '';
  }

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
