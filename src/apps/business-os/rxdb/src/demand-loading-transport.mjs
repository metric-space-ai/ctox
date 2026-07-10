// V1.5 demand-loading transport.
//
// Sits next to a CtoxWebRtcReplicationState and turns the bidirectional
// peer.request channel into a request/response demand-loading layer:
// the browser issues `rxdb.query.fetch` / `rxdb.file.fetch` requests, the
// server (Rust dispatcher) pushes correlated chunk-message frames, this
// module collects them, decodes them, and resolves the corresponding
// outstanding requestQueryFetch / requestFileFetch promise.

import { decodeChunk } from './chunk-decoder.mjs';
import { CTOX_FILE_RPC, CTOX_QUERY_RPC } from './protocol-contract.generated.mjs';

const ACK_RESPONSE = Object.freeze({ ack: true });
const SERVER_QUERY_STREAM_LIMIT = Math.max(1, Number(CTOX_QUERY_RPC.maxInFlightStreams) || 4);
const CLIENT_QUERY_STREAM_LIMIT = Math.max(1, Math.min(6, SERVER_QUERY_STREAM_LIMIT - 1 || 1));
export const CLIENT_QUERY_QUEUE_LIMIT = 128;
export const CLIENT_QUERY_QUEUE_BUDGET_BYTES = 1024 * 1024;
export const CLIENT_FILE_COLLECTOR_LIMIT = 8;
const QUERY_STREAM_LIMIT_RETRY_MS = 160;
const QUERY_STREAM_LIMIT_RETRIES = 6;
const QUERY_RATE_LIMIT_RETRY_MS = 100;
const QUERY_RATE_LIMIT_RETRIES = 16;
const QUERY_PEER_RETRY_MS = 250;
const QUERY_PEER_RETRIES = 24;
const QUERY_PEER_WAIT_TIMEOUT_MS = 8000;
const QUERY_PEER_WAIT_POLL_MS = 100;
const QUERY_FETCH_REQUEST_TIMEOUT_MS = 45000;
const DEFAULT_COLLECTOR_TIMEOUT_MS = Math.max(
  1000,
  Number(CTOX_QUERY_RPC.maxQueryRuntimeMs) + 5000 || 35000,
);
const GLOBAL_QUERY_STREAM_STATE_KEY = Symbol.for('ctox.rxdb.query-stream-state.v1');
const CANCELLED_QUERY_REQUEST_LIMIT = 256;
export const DEFAULT_FILE_COLLECTOR_BUDGET_BYTES = 512 * 1024;

/// Build the request-handler map that should be merged into
/// `createCtoxWebRtcNativePeer({ requestHandlers })`. The returned object
/// also exposes `requestQueryFetch`, `requestFileFetch` etc.
export function createDemandLoadingTransport({
  getPeerId,
  collectorTimeoutMs = DEFAULT_COLLECTOR_TIMEOUT_MS,
  fileCollectorBudgetBytes = DEFAULT_FILE_COLLECTOR_BUDGET_BYTES,
} = {}) {
  if (typeof getPeerId !== 'function') {
    throw new TypeError('createDemandLoadingTransport requires getPeerId');
  }

  const queryCollectors = new Map();   // requestId -> { chunks, resolve, reject, decoded }
  const fileCollectors = new Map();    // requestId -> { chunks, resolve, reject }
  const queryStreamState = getGlobalQueryStreamState();
  const cancelledQueryRequests = new Map();
  const transportOwner = Symbol('ctox-demand-transport-owner');
  const terminalTimeoutMs = Math.max(1, Number(collectorTimeoutMs) || DEFAULT_COLLECTOR_TIMEOUT_MS);
  const acceptedFileBudgetBytes = Math.max(
    Number(CTOX_FILE_RPC.maxBytesPerChunk) || 1,
    Number(fileCollectorBudgetBytes) || DEFAULT_FILE_COLLECTOR_BUDGET_BYTES,
  );
  const metrics = {
    queryFetchRequests: 0,
    fileFetchRequests: 0,
    queryChunksReceived: 0,
    fileChunksReceived: 0,
    queryCollectorsRejected: 0,
    fileCollectorsRejected: 0,
    queryCancelRequests: 0,
    fileCancelRequests: 0,
    maxPendingQueryCollectors: 0,
    maxPendingFileCollectors: 0,
    maxQueuedQueryRequests: 0,
    maxQueuedQueryBytes: 0,
    maxBufferedQueryChunks: 0,
    maxBufferedFileChunks: 0,
    maxBufferedFileChunkBytes: 0,
    queryCollectorTimeouts: 0,
    fileCollectorTimeouts: 0,
    fileCollectorBudgetExceeded: 0,
  };

  function updatePeaks() {
    metrics.maxPendingQueryCollectors = Math.max(metrics.maxPendingQueryCollectors, queryCollectors.size);
    metrics.maxPendingFileCollectors = Math.max(metrics.maxPendingFileCollectors, fileCollectors.size);
    metrics.maxQueuedQueryRequests = Math.max(metrics.maxQueuedQueryRequests, queryStreamState.queue.length);
    metrics.maxQueuedQueryBytes = Math.max(metrics.maxQueuedQueryBytes, queuedQueryBytes());
    metrics.maxBufferedQueryChunks = Math.max(metrics.maxBufferedQueryChunks, bufferedChunkCount(queryCollectors));
    metrics.maxBufferedFileChunks = Math.max(metrics.maxBufferedFileChunks, bufferedChunkCount(fileCollectors));
    metrics.maxBufferedFileChunkBytes = Math.max(metrics.maxBufferedFileChunkBytes, bufferedFileChunkBytes(fileCollectors));
  }

  function routeQueryChunk(chunk) {
    if (!chunk || !chunk.requestId) return;
    const slot = queryCollectors.get(chunk.requestId);
    if (!slot) return;
    slot.chunks.push(chunk);
    metrics.queryChunksReceived += 1;
    updatePeaks();
    if (chunk.complete) {
      queryCollectors.delete(chunk.requestId);
      clearCollectorTimer(slot);
      slot.resolve(slot.chunks);
    }
  }
  function routeQueryError(err) {
    if (!err || !err.requestId) return;
    const slot = queryCollectors.get(err.requestId);
    if (!slot) return;
    queryCollectors.delete(err.requestId);
    clearCollectorTimer(slot);
    metrics.queryCollectorsRejected += 1;
    const e = new Error(`${err.code || 'QUERY_ERROR'}: ${err.message || ''}`);
    e.code = err.code;
    e.retryable = Boolean(err.retryable);
    slot.reject(e);
  }
  async function routeFileChunk(chunk) {
    if (!chunk || !chunk.requestId) return;
    const slot = fileCollectors.get(chunk.requestId);
    if (!slot) return;
    const chunkBytes = typeof chunk.bytesBase64 === 'string' ? chunk.bytesBase64.length : 0;
    try {
      if (typeof slot.onChunk === 'function') {
        await slot.onChunk(chunk);
      } else {
        slot.bufferedBytes += chunkBytes;
        if (slot.bufferedBytes > acceptedFileBudgetBytes) {
          const error = new Error(`FILE_COLLECTOR_BUDGET_EXCEEDED: ${slot.bufferedBytes} > ${acceptedFileBudgetBytes}`);
          error.code = 'FILE_COLLECTOR_BUDGET_EXCEEDED';
          error.retryable = false;
          throw error;
        }
        slot.chunks.push(chunk);
      }
      metrics.fileChunksReceived += 1;
      updatePeaks();
      if (chunk.complete) {
        fileCollectors.delete(chunk.requestId);
        clearCollectorTimer(slot);
        slot.resolve(slot.chunks);
      }
    } catch (error) {
      fileCollectors.delete(chunk.requestId);
      clearCollectorTimer(slot);
      metrics.fileCollectorsRejected += 1;
      if (error?.code === 'FILE_COLLECTOR_BUDGET_EXCEEDED') {
        metrics.fileCollectorBudgetExceeded += 1;
      }
      slot.reject(error);
      Promise.resolve(
        peer?.request?.(slot.peerId, CTOX_FILE_RPC.cancel, [{
          requestId: chunk.requestId,
          reason: error?.code || 'file-chunk-consumer-failed',
        }], 2000),
      ).catch(() => {});
    }
  }
  function routeFileError(err) {
    if (!err || !err.requestId) return;
    const slot = fileCollectors.get(err.requestId);
    if (!slot) return;
    fileCollectors.delete(err.requestId);
    clearCollectorTimer(slot);
    metrics.fileCollectorsRejected += 1;
    const e = new Error(`${err.code || 'FILE_ERROR'}: ${err.message || ''}`);
    e.code = err.code;
    e.retryable = Boolean(err.retryable);
    slot.reject(e);
  }

  const requestHandlers = {
    'rxdb.query.chunk': async ({ params }) => { routeQueryChunk(params?.[0]); return ACK_RESPONSE; },
    'rxdb.query.error': async ({ params }) => { routeQueryError(params?.[0]); return ACK_RESPONSE; },
    'rxdb.file.chunk':  async ({ params }) => { await routeFileChunk(params?.[0]); return ACK_RESPONSE; },
    'rxdb.file.error':  async ({ params }) => { routeFileError(params?.[0]); return ACK_RESPONSE; },
  };

  let peer = null;
  function attach(p) { peer = p; }

  async function requestQueryFetch(envelope) {
    return withQueryStreamSlot(envelope, () => requestQueryFetchWithRetry(envelope));
  }

  function withQueryStreamSlot(envelope, fn) {
    return new Promise((resolve, reject) => {
      const requestId = String(envelope?.requestId || '');
      const estimatedBytes = estimateEnvelopeBytes(envelope);
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
      else {
        const queuedBytes = queryStreamState.queue.reduce(
          (total, entry) => total + Math.max(0, Number(entry?.estimatedBytes) || 0),
          0,
        );
        if (
          queryStreamState.queue.length >= CLIENT_QUERY_QUEUE_LIMIT
          || queuedBytes + estimatedBytes > CLIENT_QUERY_QUEUE_BUDGET_BYTES
        ) {
          const error = new Error('QUERY_QUEUE_LIMIT: queued demand requests exceed the browser count/byte budget');
          error.code = 'QUERY_QUEUE_LIMIT';
          error.retryable = true;
          reject(error);
          return;
        }
        queryStreamState.queue.push({ requestId, run, reject, owner: transportOwner, estimatedBytes });
        updatePeaks();
      }
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
        const rateLimited = isRetryableQueryRateLimited(error);
        const retryLimit = peerUnavailable
          ? QUERY_PEER_RETRIES
          : rateLimited
            ? QUERY_RATE_LIMIT_RETRIES
            : QUERY_STREAM_LIMIT_RETRIES;
        if (!isRetryableQueryFetch(error) || attempt >= retryLimit) {
          throw error;
        }
        attempt += 1;
        const retryDelayMs = peerUnavailable
          ? QUERY_PEER_RETRY_MS
          : rateLimited
            ? QUERY_RATE_LIMIT_RETRY_MS
            : QUERY_STREAM_LIMIT_RETRY_MS;
        await delay(retryDelayMs * attempt);
      }
    }
  }

  async function requestQueryFetchOnce(envelope) {
    const requestId = envelope?.requestId;
    const cancelReason = consumeQueryCancelReason(requestId);
    if (cancelReason) throw createQueryCancelError(cancelReason);
    if (!peer) throw new Error('demand transport has no peer attached');
    const peerId = await waitForPeerId();
    if (!peerId) throw new Error('PEER_UNAVAILABLE');
    const promise = new Promise((resolve, reject) => {
      queryCollectors.set(requestId, { chunks: [], resolve, reject, peerId });
      metrics.queryFetchRequests += 1;
      updatePeaks();
    });
    try {
      await peer.request(peerId, CTOX_QUERY_RPC.fetch, [envelope], QUERY_FETCH_REQUEST_TIMEOUT_MS);
    } catch (err) {
      clearCollectorTimer(queryCollectors.get(requestId));
      queryCollectors.delete(requestId);
      throw err;
    }
    armCollectorTimeout(queryCollectors, requestId, 'query', peerId, CTOX_QUERY_RPC.cancel);
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
      || isRetryableQueryRateLimited(error)
      || isRetryableQueryPeerUnavailable(error);
  }

  function isRetryableQueryRateLimited(error) {
    const code = String(error?.code || '');
    const message = String(error?.message || '');
    return Boolean(error?.retryable)
      && (code === 'RATE_LIMITED' || message.includes('RATE_LIMITED'));
  }

  function isRetryableQueryPeerUnavailable(error) {
    const message = String(error?.message || '');
    return message === 'PEER_UNAVAILABLE'
      || /WebRTC peer .* is not open/.test(message)
      || message.includes('Timed out waiting for WebRTC response rxdb.query.fetch');
  }

  function delay(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  async function requestQueryCancel({ requestId, reason = 'client-abort' }) {
    if (!requestId) return;
    metrics.queryCancelRequests += 1;
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

  async function requestFileFetch({ requestId, fileId, range, knownSequences, collectionName, onChunk }) {
    if (!peer) throw new Error('demand transport has no peer attached');
    if (fileCollectors.size >= CLIENT_FILE_COLLECTOR_LIMIT) {
      const error = new Error('FILE_COLLECTOR_LIMIT: too many active browser file collectors');
      error.code = 'FILE_COLLECTOR_LIMIT';
      error.retryable = true;
      throw error;
    }
    const peerId = await waitForPeerId();
    if (!peerId) throw new Error('PEER_UNAVAILABLE');
    const promise = new Promise((resolve, reject) => {
      fileCollectors.set(requestId, {
        chunks: [],
        resolve,
        reject,
        peerId,
        onChunk: typeof onChunk === 'function' ? onChunk : null,
        bufferedBytes: 0,
      });
      metrics.fileFetchRequests += 1;
      updatePeaks();
    });
    try {
      await peer.request(peerId, CTOX_FILE_RPC.fetch, [{
        requestId,
        collectionName,
        fileId,
        range: range ?? null,
        knownSequences: knownSequences ?? [],
      }]);
    } catch (err) {
      clearCollectorTimer(fileCollectors.get(requestId));
      fileCollectors.delete(requestId);
      throw err;
    }
    armCollectorTimeout(fileCollectors, requestId, 'file', peerId, CTOX_FILE_RPC.cancel);
    const chunks = await promise;
    return chunks.map((c) => ({ sequence: c.sequence, bytesBase64: c.bytesBase64, hash: c.hash }));
  }

  async function requestFileCancel({ requestId, reason = 'client-abort' } = {}) {
    if (!requestId) return false;
    metrics.fileCancelRequests += 1;
    const slot = fileCollectors.get(requestId);
    const error = createFileCancelError(reason);
    let cancelled = false;
    if (slot) {
      fileCollectors.delete(requestId);
      clearCollectorTimer(slot);
      metrics.fileCollectorsRejected += 1;
      slot.reject(error);
      cancelled = true;
    }
    const peerId = slot?.peerId || (peer ? resolvePeerId() : '');
    if (peer && peerId) {
      try {
        await peer.request(peerId, CTOX_FILE_RPC.cancel, [{ requestId, reason }], 2000);
      } catch {
        // best-effort
      }
    }
    return cancelled;
  }

  function abortPeerRequests(peerId, reason = 'peer-close') {
    const queryError = createQueryCancelError(reason);
    const fileError = createFileCancelError(reason);
    let rejected = 0;
    for (const [requestId, slot] of [...queryCollectors.entries()]) {
      if (peerId && slot.peerId !== peerId) continue;
      queryCollectors.delete(requestId);
      clearCollectorTimer(slot);
      metrics.queryCollectorsRejected += 1;
      slot.reject(queryError);
      rejected += 1;
    }
    for (const [requestId, slot] of [...fileCollectors.entries()]) {
      if (peerId && slot.peerId !== peerId) continue;
      fileCollectors.delete(requestId);
      clearCollectorTimer(slot);
      metrics.fileCollectorsRejected += 1;
      slot.reject(fileError);
      rejected += 1;
    }
    rejected += rejectQueuedQueryRequestsForOwner(reason);
    return rejected;
  }

  function pendingQueryCount() { return queryCollectors.size + queryStreamState.queue.length; }
  function pendingFileCount() { return fileCollectors.size; }
  function diagnostics() {
    updatePeaks();
    return {
      schema: 'ctox.rxdb.demand_transport.v1',
      pendingQueryCollectors: queryCollectors.size,
      pendingFileCollectors: fileCollectors.size,
      queuedQueryRequests: queryStreamState.queue.length,
      queuedQueryBytes: queuedQueryBytes(),
      activeQueryStreams: queryStreamState.active,
      bufferedQueryChunks: bufferedChunkCount(queryCollectors),
      bufferedFileChunks: bufferedChunkCount(fileCollectors),
      bufferedFileChunkBytes: bufferedFileChunkBytes(fileCollectors),
      fileCollectorBudgetBytes: acceptedFileBudgetBytes,
      cancelledQueryRequestCacheSize: cancelledQueryRequests.size,
      ...metrics,
    };
  }

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
    clearCollectorTimer(slot);
    metrics.queryCollectorsRejected += 1;
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
        metrics.queryCollectorsRejected += 1;
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

  function rejectQueuedQueryRequestsForOwner(reason) {
    const remaining = [];
    let rejected = 0;
    const error = createQueryCancelError(reason);
    for (const entry of queryStreamState.queue) {
      if (entry?.owner === transportOwner) {
        rejected += 1;
        metrics.queryCollectorsRejected += 1;
        entry.reject(error);
      } else {
        remaining.push(entry);
      }
    }
    if (rejected) {
      queryStreamState.queue.splice(0, queryStreamState.queue.length, ...remaining);
    }
    return rejected;
  }

  function queuedQueryRequestId(entry) {
    if (!entry || typeof entry === 'function') return '';
    return String(entry.requestId || '');
  }

  function queuedQueryBytes() {
    return queryStreamState.queue.reduce(
      (total, entry) => total + Math.max(0, Number(entry?.estimatedBytes) || 0),
      0,
    );
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

  function createFileCancelError(reason) {
    const error = new Error(`FILE_CANCELLED: ${reason || 'client-abort'}`);
    error.code = 'FILE_CANCELLED';
    error.retryable = false;
    return error;
  }

  function armCollectorTimeout(collectors, requestId, kind, peerId, cancelMethod) {
    const slot = collectors.get(requestId);
    if (!slot || slot.timer) return;
    slot.timer = setTimeout(() => {
      if (collectors.get(requestId) !== slot) return;
      collectors.delete(requestId);
      if (kind === 'query') {
        metrics.queryCollectorsRejected += 1;
        metrics.queryCollectorTimeouts += 1;
      } else {
        metrics.fileCollectorsRejected += 1;
        metrics.fileCollectorTimeouts += 1;
      }
      const error = new Error(`${kind.toUpperCase()}_COLLECTOR_TIMEOUT: terminal frame missing`);
      error.code = `${kind.toUpperCase()}_COLLECTOR_TIMEOUT`;
      error.retryable = true;
      slot.reject(error);
      Promise.resolve(
        peer?.request?.(peerId, cancelMethod, [{ requestId, reason: 'collector-timeout' }], 2000),
      ).catch(() => {});
    }, terminalTimeoutMs);
  }

  function clearCollectorTimer(slot) {
    if (slot?.timer) {
      clearTimeout(slot.timer);
      slot.timer = null;
    }
  }

  function estimateEnvelopeBytes(envelope) {
    let encoded = '';
    try { encoded = JSON.stringify(envelope ?? null); } catch { return CLIENT_QUERY_QUEUE_BUDGET_BYTES + 1; }
    if (typeof TextEncoder === 'function') return new TextEncoder().encode(encoded).byteLength;
    return encoded.length * 2;
  }

  function resolvePeerId() {
    const configured = getPeerId();
    if (configured && isPeerOpen(configured)) return configured;
    return firstOpenPeerId();
  }

  function firstOpenPeerId() {
    const entries = peer?.connections?.entries?.();
    if (!entries) return '';
    for (const [peerId, connection] of entries) {
      if (isPeerConnectionOpen(connection)) return peerId;
    }
    return '';
  }

  function isPeerOpen(peerId) {
    return Boolean(peerId && isPeerConnectionOpen(peer?.connections?.get?.(peerId)));
  }

  function isPeerConnectionOpen(connection) {
    if (!connection) return false;
    const channelState = connection?.channel?.readyState || connection?.channelReadyState || '';
    const peerState = connection?.peer?.connectionState
      || connection?.peerConnectionState
      || connection?.connectionState
      || '';
    return channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(String(peerState));
  }

  async function waitForPeerId(timeoutMs = QUERY_PEER_WAIT_TIMEOUT_MS) {
    const immediate = resolvePeerId();
    if (immediate) return immediate;
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      await delay(QUERY_PEER_WAIT_POLL_MS);
      const peerId = resolvePeerId();
      if (peerId) return peerId;
    }
    return '';
  }

  return {
    requestHandlers,
    attach,
    requestQueryFetch,
    requestQueryCancel,
    requestFileFetch,
    requestFileCancel,
    abortPeerRequests,
    pendingQueryCount,
    pendingFileCount,
    diagnostics,
  };
}

function bufferedChunkCount(collectors) {
  let total = 0;
  for (const slot of collectors.values()) {
    total += Array.isArray(slot?.chunks) ? slot.chunks.length : 0;
  }
  return total;
}

function bufferedFileChunkBytes(collectors) {
  let total = 0;
  for (const slot of collectors.values()) {
    if (!Array.isArray(slot?.chunks)) continue;
    for (const chunk of slot.chunks) {
      if (typeof chunk?.bytesBase64 === 'string') {
        total += chunk.bytesBase64.length;
      }
    }
  }
  return total;
}

function getGlobalQueryStreamState() {
  if (!globalThis[GLOBAL_QUERY_STREAM_STATE_KEY]) {
    globalThis[GLOBAL_QUERY_STREAM_STATE_KEY] = { active: 0, queue: [] };
  }
  return globalThis[GLOBAL_QUERY_STREAM_STATE_KEY];
}
