import { cloneQueryRow } from './query-projection.mjs';
import { CTOX_QUERY_RPC } from './protocol-contract.generated.mjs';

export const PROJECTED_QUERY_CACHE_BUDGET_BYTES = 16 * 1024 * 1024;
export const PROJECTED_QUERY_CACHE_MAX_WINDOWS = 64;
export const PROJECTED_QUERY_WINDOW_MAX_BYTES = CTOX_QUERY_RPC.projectedWindowMaxBytes;
export const PROJECTED_QUERY_WINDOW_MAX_ROWS = CTOX_QUERY_RPC.projectedWindowMaxRows;
const ROWS = 'rows';
const META = 'metadata';
const BUDGET_KEY = '@budget';

function cacheRecord(key, documents, now) {
  if (typeof key !== 'string' || !key || key === BUDGET_KEY) throw new TypeError('invalid projected window key');
  if (!Array.isArray(documents) || documents.length > PROJECTED_QUERY_WINDOW_MAX_ROWS) throw new TypeError('projected windows contain at most 200 rows');
  const json = JSON.stringify(documents);
  const bytes = new TextEncoder().encode(json).byteLength;
  if (bytes > PROJECTED_QUERY_WINDOW_MAX_BYTES) {
    throw Object.assign(new Error('PROJECTED_QUERY_WINDOW_TOO_LARGE: narrow fields or the page size'), {
      code: 'PROJECTED_QUERY_WINDOW_TOO_LARGE', retryable: false,
    });
  }
  return { key, documents: JSON.parse(json), bytes, lastAccessedAt: now };
}

export function createMemoryProjectedQueryCache() {
  const rows = new Map();
  let bytes = 0;
  return {
    name: 'memory',
    async put(key, documents, now = Date.now()) {
      const record = cacheRecord(key, documents, now);
      bytes += record.bytes - (rows.get(key)?.bytes || 0);
      rows.set(key, record);
      const oldest = [...rows.values()].sort((a, b) => a.lastAccessedAt - b.lastAccessedAt);
      for (const candidate of oldest) {
        if (bytes <= PROJECTED_QUERY_CACHE_BUDGET_BYTES && rows.size <= PROJECTED_QUERY_CACHE_MAX_WINDOWS) break;
        if (candidate.key === key) continue;
        rows.delete(candidate.key);
        bytes -= candidate.bytes;
      }
    },
    async get(key, now = Date.now()) {
      const record = rows.get(key);
      if (!record) return null;
      record.lastAccessedAt = now;
      return cloneQueryRow(record.documents);
    },
    async clear() { rows.clear(); bytes = 0; },
    async close() {},
  };
}

// Payloads have their own small database. Existing sidecar query-window scans
// read membership metadata only; they never clone every projected row. This
// also leaves the current sidecar schema/version and its legacy tabs intact.
export function createIndexedDbProjectedQueryCache({ databaseName }) {
  if (typeof databaseName !== 'string' || !databaseName) throw new TypeError('projected cache requires databaseName');
  let opening = null;
  let fallback = null;
  async function open() {
    if (!opening) {
      opening = new Promise((resolve, reject) => {
        if (!globalThis.indexedDB) { reject(new Error('IndexedDB unavailable')); return; }
        let settled = false;
        const finish = (fn, value) => {
          if (settled) return;
          settled = true; clearTimeout(timer); fn(value);
        };
        const timer = setTimeout(() => finish(reject, new Error('Projected cache open timed out')), 4000);
        const request = indexedDB.open(databaseName, 1);
        request.onupgradeneeded = () => {
          const db = request.result;
          db.createObjectStore(ROWS, { keyPath: 'key' });
          const metadata = db.createObjectStore(META, { keyPath: 'key' });
          metadata.createIndex('lastAccessedAt', 'lastAccessedAt');
        };
        request.onsuccess = () => {
          if (settled) { request.result.close(); return; }
          const db = request.result;
          db.onversionchange = () => { db.close(); opening = null; };
          finish(resolve, db);
        };
        request.onerror = () => finish(reject, request.error || new Error('Projected cache open failed'));
        request.onblocked = () => finish(reject, new Error('Projected cache open blocked'));
      }).catch((error) => { opening = null; throw error; });
    }
    return opening;
  }
  async function backend() {
    if (fallback) return null;
    try { return await open(); }
    catch { fallback ||= createMemoryProjectedQueryCache(); return null; }
  }
  return {
    get name() { return fallback ? 'memory-fallback' : 'indexeddb'; },
    async put(key, documents, now = Date.now()) {
      const record = cacheRecord(key, documents, now);
      const db = await backend();
      if (!db) return fallback.put(key, documents, now);
      // Payload + byte/count accounting + LRU eviction commit in ONE database
      // transaction, including across tabs. The eviction cursor reads only
      // small metadata records, never payload blobs.
      await new Promise((resolve, reject) => {
        const tx = db.transaction([ROWS, META], 'readwrite');
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error || new Error('Projected cache write failed'));
        tx.onabort = () => reject(tx.error || new Error('Projected cache write aborted'));
        const rowStore = tx.objectStore(ROWS), metaStore = tx.objectStore(META);
        const budgetRequest = metaStore.get(BUDGET_KEY);
        budgetRequest.onsuccess = () => {
          const previousRequest = metaStore.get(key);
          previousRequest.onsuccess = () => {
            const previous = previousRequest.result;
            const budget = budgetRequest.result || { bytes: 0, count: 0 };
            let bytes = budget.bytes + record.bytes - (previous?.bytes || 0);
            let count = budget.count + (previous ? 0 : 1);
            rowStore.put({ key, documents: record.documents });
            metaStore.put({ key, bytes: record.bytes, lastAccessedAt: now });
            const finishBudget = () => metaStore.put({ key: BUDGET_KEY, bytes, count });
            if (bytes <= PROJECTED_QUERY_CACHE_BUDGET_BYTES && count <= PROJECTED_QUERY_CACHE_MAX_WINDOWS) {
              finishBudget(); return;
            }
            const cursorRequest = metaStore.index('lastAccessedAt').openCursor();
            cursorRequest.onsuccess = () => {
              if (bytes <= PROJECTED_QUERY_CACHE_BUDGET_BYTES && count <= PROJECTED_QUERY_CACHE_MAX_WINDOWS) {
                finishBudget(); return;
              }
              const cursor = cursorRequest.result;
              if (!cursor) { tx.abort(); return; }
              if (cursor.value.key !== key) {
                bytes -= cursor.value.bytes; count -= 1;
                rowStore.delete(cursor.value.key); cursor.delete();
              }
              cursor.continue();
            };
          };
        };
      });
    },
    async get(key, now = Date.now()) {
      if (!key) return null;
      const db = await backend();
      if (!db) return fallback.get(key, now);
      return new Promise((resolve, reject) => {
        const tx = db.transaction([ROWS, META], 'readwrite');
        let documents = null;
        tx.oncomplete = () => resolve(documents);
        tx.onerror = () => reject(tx.error || new Error('Projected cache read failed'));
        tx.onabort = () => reject(tx.error || new Error('Projected cache read aborted'));
        const rowRequest = tx.objectStore(ROWS).get(key);
        rowRequest.onsuccess = () => {
          documents = rowRequest.result?.documents || null;
          if (!documents) return;
          const metadata = tx.objectStore(META);
          const metaRequest = metadata.get(key);
          metaRequest.onsuccess = () => {
            if (metaRequest.result) metadata.put({ ...metaRequest.result, lastAccessedAt: now });
          };
        };
      });
    },
    async clear() {
      const db = await backend();
      if (!db) return fallback.clear();
      await new Promise((resolve, reject) => {
        const tx = db.transaction([ROWS, META], 'readwrite');
        tx.oncomplete = () => resolve();
        tx.onerror = tx.onabort = () => reject(tx.error || new Error('Projected cache clear failed'));
        tx.objectStore(ROWS).clear(); tx.objectStore(META).clear();
      });
    },
    async close() {
      const current = opening; opening = null;
      if (current) { try { (await current).close(); } catch {} }
      await fallback?.close(); fallback = null;
    },
  };
}
