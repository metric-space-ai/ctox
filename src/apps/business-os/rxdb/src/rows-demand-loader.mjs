// Knowledge-table row windows over rxdb.rows.*.
//
// Results stay in memory. This loader never writes a collection or IndexedDB:
// catalog documents remain the only replicated knowledge_tables records.

import { CTOX_ROWS_RPC } from './protocol-contract.generated.mjs';

export const ROWS_RESULT_CACHE_BUDGET_BYTES = 8 * 1024 * 1024;

export function createRowsDemandLoader({
  transport,
  collectionName = 'knowledge_tables',
} = {}) {
  if (!transport || typeof transport.fetchRows !== 'function') {
    throw new TypeError('rows loader requires transport.fetchRows');
  }

  const inflight = new Map();
  const cache = new Map();
  const latestHashByTable = new Map();
  let cacheBytes = 0;
  let requestSequence = 0;

  function fetchRows(tableId, { offset = 0, limit = CTOX_ROWS_RPC.maxRowsPerWindow, signal } = {}) {
    const normalizedOffset = normalizeOffset(offset);
    const normalizedLimit = normalizeLimit(limit);
    const requestId = `rows-${Date.now()}-${requestSequence += 1}`;
    const controller = new AbortController();
    const slot = { controller, tableId };
    const onCallerAbort = () => controller.abort(signal?.reason || 'client-abort');
    if (signal) {
      if (signal.aborted) controller.abort(signal.reason || 'client-abort');
      else signal.addEventListener('abort', onCallerAbort, { once: true });
    }
    inflight.set(requestId, slot);
    const job = (async () => {
      try {
        if (controller.signal.aborted) throw cancellationError(controller.signal.reason);
        const cached = readCache(tableId, normalizedOffset, normalizedLimit);
        if (cached) return cached;
        const window = await transport.fetchRows({
          collection: collectionName,
          tableId,
          offset: normalizedOffset,
          limit: normalizedLimit,
          signal: controller.signal,
          requestId,
        });
        const result = {
          rows: Array.isArray(window?.rows) ? window.rows : [],
          rowCount: finiteCount(window?.rowCount),
          contentHash: window?.contentHash ?? null,
          schemaHash: window?.schemaHash ?? null,
          offset: Number.isFinite(Number(window?.offset)) ? Number(window.offset) : normalizedOffset,
        };
        remember(tableId, result, normalizedOffset, normalizedLimit);
        return result;
      } finally {
        signal?.removeEventListener?.('abort', onCallerAbort);
        if (inflight.get(requestId) === slot) inflight.delete(requestId);
      }
    })();
    slot.promise = job;
    return job;
  }

  async function fetchAllRows(tableId, {
    pageSize = CTOX_ROWS_RPC.maxRowsPerWindow,
    signal,
  } = {}) {
    const limit = normalizeLimit(pageSize);
    const rows = [];
    let offset = 0;
    let rowCount = null;
    let contentHash = null;
    let schemaHash = null;
    for (;;) {
      if (signal?.aborted) throw createCancelError(signal.reason || 'client-abort');
      const page = await fetchRows(tableId, { offset, limit, signal });
      if (rowCount == null) rowCount = page.rowCount;
      if (contentHash == null) contentHash = page.contentHash;
      else if (page.contentHash != null && page.contentHash !== contentHash) {
        const error = new Error('ROWS_SOURCE_ERROR: content hash changed while paging');
        error.code = 'ROWS_SOURCE_ERROR';
        error.retryable = true;
        throw error;
      }
      schemaHash = page.schemaHash ?? schemaHash;
      const pageRows = Array.isArray(page.rows) ? page.rows : [];
      if (pageRows.length === 0) {
        if (rowCount != null && offset < rowCount) {
          const error = new Error('ROWS_SOURCE_ERROR: row window ended before rowCount');
          error.code = 'ROWS_SOURCE_ERROR';
          error.retryable = true;
          throw error;
        }
        break;
      }
      rows.push(...pageRows);
      offset += pageRows.length;
      if (rowCount != null && offset >= rowCount) break;
      if (pageRows.length < limit) break;
    }
    return {
      rows,
      rowCount: rowCount == null ? rows.length : rowCount,
      contentHash,
      schemaHash,
      offset: 0,
    };
  }

  function abortAllInFlight(reason = 'client-abort') {
    const slots = [...inflight.values()];
    inflight.clear();
    for (const slot of slots) {
      try { slot.promise?.catch?.(() => {}); } catch { /* already rejected */ }
      try { slot.controller.abort(reason); } catch { /* best-effort */ }
    }
    return slots.length;
  }

  function readCache(tableId, offset, limit) {
    const hash = latestHashByTable.get(normalizeRowsTableId(tableId));
    if (!hash) return null;
    const key = cacheKey(tableId, hash, offset, limit);
    const entry = cache.get(key);
    if (!entry) return null;
    cache.delete(key);
    cache.set(key, entry);
    return entry.value;
  }

  function remember(tableId, result, offset, limit) {
    const hash = result?.contentHash == null ? '' : String(result.contentHash);
    if (!hash) return;
    const normalizedId = normalizeRowsTableId(tableId);
    const previous = latestHashByTable.get(normalizedId);
    if (previous && previous !== hash) dropTable(normalizedId);
    latestHashByTable.set(normalizedId, hash);
    const bytes = estimateBytes(result.rows);
    if (bytes > ROWS_RESULT_CACHE_BUDGET_BYTES) return;
    const key = cacheKey(tableId, hash, offset, limit);
    if (cache.has(key)) {
      cacheBytes -= cache.get(key).bytes;
      cache.delete(key);
    }
    cache.set(key, { value: result, bytes, tableId: normalizedId });
    cacheBytes += bytes;
    while (cacheBytes > ROWS_RESULT_CACHE_BUDGET_BYTES && cache.size > 0) {
      const oldest = cache.keys().next().value;
      const entry = cache.get(oldest);
      cache.delete(oldest);
      cacheBytes -= entry?.bytes || 0;
    }
  }

  function dropTable(normalizedId) {
    for (const [key, entry] of [...cache.entries()]) {
      if (entry?.tableId !== normalizedId) continue;
      cache.delete(key);
      cacheBytes -= entry.bytes || 0;
    }
  }

  return {
    fetchRows,
    fetchAllRows,
    abortAllInFlight,
  };
}

function cacheKey(tableId, contentHash, offset, limit) {
  return `${normalizeRowsTableId(tableId)}\0${contentHash}\0${offset}\0${limit}`;
}

function normalizeRowsTableId(tableId) {
  const raw = String(tableId || '').trim();
  return raw.startsWith('table:') ? raw.slice('table:'.length) : raw;
}

function normalizeOffset(offset) {
  const value = Number(offset);
  if (!Number.isFinite(value) || value <= 0) return 0;
  return Math.floor(value);
}

function normalizeLimit(limit) {
  const cap = Math.max(1, Number(CTOX_ROWS_RPC.maxRowsPerWindow) || 1);
  const value = Number(limit);
  if (!Number.isFinite(value) || value <= 0) return cap;
  return Math.min(cap, Math.floor(value));
}

function finiteCount(value) {
  const number = Number(value);
  return Number.isFinite(number) && number >= 0 ? number : 0;
}

function estimateBytes(rows) {
  try { return JSON.stringify(rows ?? []).length; } catch { return ROWS_RESULT_CACHE_BUDGET_BYTES + 1; }
}

function cancellationError(reason) {
  if (reason && typeof reason === 'object' && reason.code === 'ROWS_CANCELLED') return reason;
  return createCancelError(typeof reason === 'string' ? reason : 'client-abort');
}

function createCancelError(reason) {
  const text = typeof reason === 'string' && reason.trim() ? reason.trim() : 'client-abort';
  const error = new Error(`ROWS_CANCELLED: ${text}`);
  error.name = 'AbortError';
  error.code = 'ROWS_CANCELLED';
  error.retryable = false;
  return error;
}
