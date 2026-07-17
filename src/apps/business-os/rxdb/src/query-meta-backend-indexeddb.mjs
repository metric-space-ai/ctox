// IndexedDB backend for the V1.5 sidecar database. Lazy-open: only the first
// call to any backend method triggers `indexedDB.open`. The primary documents
// database (ctox_business_os_js_v1) is NOT touched here.

import { createMemoryMetaBackend } from './query-meta-backend-memory.mjs';

const SIDECAR_DB_VERSION = 2;
const STORE_QUERY_WINDOWS = 'queryWindows';
const STORE_QUERY_WINDOW_REFS = 'queryWindowRefs';
const STORE_DOCUMENT_ACCESS = 'documentAccess';
const STORE_CACHE_STATS = 'cacheStats';
const OPEN_TIMEOUT_MS = 4000;

export function createIndexedDbMetaBackend({ databaseName }) {
  if (!databaseName) throw new TypeError('createIndexedDbMetaBackend requires databaseName');
  let dbPromise = null;
  let fallbackBackend = null;
  const fallback = () => {
    if (!fallbackBackend) fallbackBackend = createMemoryMetaBackend();
    return fallbackBackend;
  };
  const open = async () => {
    if (!dbPromise) {
      dbPromise = Promise.resolve()
        .then(() => openSidecarDatabase(databaseName))
        .catch((error) => {
          dbPromise = null;
          throw markSidecarOpenError(error);
        });
    }
    return dbPromise;
  };
  const withDb = async (method, args, operation) => {
    if (fallbackBackend) return fallbackBackend[method](...args);
    try {
      return await operation(await open());
    } catch (error) {
      if (!isSidecarOpenError(error)) throw error;
      return fallback()[method](...args);
    }
  };

  return {
    get name() {
      return fallbackBackend ? 'memory-fallback' : 'indexeddb';
    },
    async putQueryWindow(record) {
      await withDb(
        'putQueryWindow',
        [record],
        (db) => runRequest(
          db
            .transaction(STORE_QUERY_WINDOWS, 'readwrite')
            .objectStore(STORE_QUERY_WINDOWS)
            .put(record),
        ),
      );
    },
    async getQueryWindow(key) {
      return withDb(
        'getQueryWindow',
        [key],
        (db) => runRequest(
          db
            .transaction(STORE_QUERY_WINDOWS, 'readonly')
            .objectStore(STORE_QUERY_WINDOWS)
            .get(parseQueryWindowKey(key)),
        ),
      );
    },
    async deleteQueryWindow(key) {
      await withDb(
        'deleteQueryWindow',
        [key],
        async (db) => {
          await deleteQueryWindowRefs(db, stringKey(parseQueryWindowKey(key)));
          await runRequest(
            db
              .transaction(STORE_QUERY_WINDOWS, 'readwrite')
              .objectStore(STORE_QUERY_WINDOWS)
              .delete(parseQueryWindowKey(key)),
          );
        },
      );
    },
    async scanQueryWindows() {
      return withDb(
        'scanQueryWindows',
        [],
        (db) => runRequest(
          db
            .transaction(STORE_QUERY_WINDOWS, 'readonly')
            .objectStore(STORE_QUERY_WINDOWS)
            .getAll(),
        ),
      );
    },
    async replaceQueryWindowDocumentRefs(record) {
      await withDb(
        'replaceQueryWindowDocumentRefs',
        [record],
        async (db) => {
          const windowKey = queryWindowKey(record);
          await deleteQueryWindowRefs(db, windowKey);
          await putQueryWindowRefs(db, record);
        },
      );
    },
    async getQueryWindowKeysByDocumentIds(collection, ids) {
      const normalizedIds = normalizeDocumentIds(ids);
      if (!normalizedIds.length) return [];
      return withDb(
        'getQueryWindowKeysByDocumentIds',
        [collection, ids],
        async (db) => {
          const tx = db.transaction(STORE_QUERY_WINDOW_REFS, 'readonly');
          const index = tx.objectStore(STORE_QUERY_WINDOW_REFS).index('collection_documentId');
          const requests = normalizedIds.map((id) => runRequest(index.getAll([collection, id])));
          const rowsByDocument = await Promise.all(requests);
          const keys = new Set();
          for (const rows of rowsByDocument) {
            for (const row of rows || []) {
              if (row?.windowKey) keys.add(row.windowKey);
            }
          }
          return Array.from(keys);
        }
      );
    },
    async putDocumentAccess(record) {
      await withDb(
        'putDocumentAccess',
        [record],
        (db) => runRequest(
          db
            .transaction(STORE_DOCUMENT_ACCESS, 'readwrite')
            .objectStore(STORE_DOCUMENT_ACCESS)
            .put(record),
        ),
      );
    },
    async getDocumentAccess(collection, id) {
      return withDb(
        'getDocumentAccess',
        [collection, id],
        (db) => runRequest(
          db
            .transaction(STORE_DOCUMENT_ACCESS, 'readonly')
            .objectStore(STORE_DOCUMENT_ACCESS)
            .get([collection, id]),
        ),
      );
    },
    async deleteDocumentAccess(collection, id) {
      await withDb(
        'deleteDocumentAccess',
        [collection, id],
        (db) => runRequest(
          db
            .transaction(STORE_DOCUMENT_ACCESS, 'readwrite')
            .objectStore(STORE_DOCUMENT_ACCESS)
            .delete([collection, id]),
        ),
      );
    },
    async scanDocumentAccess() {
      return withDb(
        'scanDocumentAccess',
        [],
        (db) => runRequest(
          db
            .transaction(STORE_DOCUMENT_ACCESS, 'readonly')
            .objectStore(STORE_DOCUMENT_ACCESS)
            .getAll(),
        ),
      );
    },
    async putCacheStats(record) {
      await withDb(
        'putCacheStats',
        [record],
        (db) => runRequest(
          db
            .transaction(STORE_CACHE_STATS, 'readwrite')
            .objectStore(STORE_CACHE_STATS)
            .put(record),
        ),
      );
    },
    async getCacheStats(databaseName) {
      return withDb(
        'getCacheStats',
        [databaseName],
        (db) => runRequest(
          db
            .transaction(STORE_CACHE_STATS, 'readonly')
            .objectStore(STORE_CACHE_STATS)
            .get(databaseName),
        ),
      );
    },
    async clear() {
      await withDb(
        'clear',
        [],
        async (db) => {
          for (const name of [STORE_QUERY_WINDOWS, STORE_QUERY_WINDOW_REFS, STORE_DOCUMENT_ACCESS, STORE_CACHE_STATS]) {
            await runRequest(db.transaction(name, 'readwrite').objectStore(name).clear());
          }
        },
      );
    },
    async close() {
      const currentDbPromise = dbPromise;
      dbPromise = null;
      if (currentDbPromise) {
        try {
          const db = await currentDbPromise;
          db.close();
        } catch {
          // Open failures already switched the sidecar to memory for this session.
        }
      }
      await fallbackBackend?.close?.();
      fallbackBackend = null;
    },
  };
}

function openSidecarDatabase(databaseName) {
  if (!globalThis.indexedDB) {
    throw new Error('indexedDB is required for sidecar metadata storage');
  }
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (fn, value) => {
      if (settled) return false;
      settled = true;
      clearTimeout(timer);
      fn(value);
      return true;
    };
    const timer = setTimeout(() => {
      finish(reject, new Error(`IndexedDB open timed out for sidecar ${databaseName}`));
    }, OPEN_TIMEOUT_MS);
    const request = globalThis.indexedDB.open(databaseName, SIDECAR_DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_QUERY_WINDOWS)) {
        const store = db.createObjectStore(STORE_QUERY_WINDOWS, {
          keyPath: ['collection', 'queryFingerprint', 'offset', 'limit'],
        });
        store.createIndex('collection', 'collection', { unique: false });
        store.createIndex('collection_lastAccessedAt', ['collection', 'lastAccessedAt'], {
          unique: false,
        });
      }
      if (!db.objectStoreNames.contains(STORE_QUERY_WINDOW_REFS)) {
        const store = db.createObjectStore(STORE_QUERY_WINDOW_REFS, {
          keyPath: ['collection', 'documentId', 'windowKey'],
        });
        store.createIndex('collection_documentId', ['collection', 'documentId'], {
          unique: false,
        });
        store.createIndex('windowKey', 'windowKey', { unique: false });
      }
      if (!db.objectStoreNames.contains(STORE_DOCUMENT_ACCESS)) {
        const store = db.createObjectStore(STORE_DOCUMENT_ACCESS, {
          keyPath: ['collection', 'id'],
        });
        store.createIndex('collection_lastAccessedAt', ['collection', 'lastAccessedAt'], {
          unique: false,
        });
      }
      if (!db.objectStoreNames.contains(STORE_CACHE_STATS)) {
        db.createObjectStore(STORE_CACHE_STATS, { keyPath: 'databaseName' });
      }
    };
    request.onsuccess = () => {
      const db = request.result;
      if (!finish(resolve, db)) {
        try { db.close(); } catch {}
      }
    };
    request.onerror = () => {
      finish(reject, request.error || new Error(`failed to open sidecar ${databaseName}`));
    };
    request.onblocked = () => {
      finish(reject, new Error(`IndexedDB open blocked for sidecar ${databaseName}`));
    };
  });
}

function markSidecarOpenError(error) {
  if (error && typeof error === 'object') {
    try { error.ctoxSidecarOpenError = true; } catch {}
    return error;
  }
  const wrapped = new Error(String(error || 'sidecar IndexedDB open failed'));
  wrapped.ctoxSidecarOpenError = true;
  return wrapped;
}

function isSidecarOpenError(error) {
  return Boolean(error?.ctoxSidecarOpenError);
}

function parseQueryWindowKey(key) {
  if (Array.isArray(key)) return key;
  if (typeof key === 'string') {
    const parts = key.split('|');
    if (parts.length !== 4) throw new TypeError(`invalid query window key: ${key}`);
    const [collection, fingerprint, offset, limit] = parts;
    return [collection, fingerprint, Number(offset), Number(limit)];
  }
  throw new TypeError('query window key must be array or string');
}

function runRequest(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function queryWindowKey(record) {
  return [record.collection, record.queryFingerprint, record.offset, record.limit].join('|');
}

function stringKey(key) {
  if (Array.isArray(key)) return key.join('|');
  if (typeof key === 'string') return key;
  throw new TypeError('query window key must be array or string');
}

function normalizeDocumentIds(ids) {
  if (!Array.isArray(ids)) return [];
  return Array.from(new Set(ids.map((id) => String(id || '')).filter(Boolean)));
}

async function putQueryWindowRefs(db, record) {
  // SYNC-52: real document ids plus synthetic selector ref ids
  // ($nonsimple / $field|<path>) share the collection_documentId index so the
  // change-invalidation path resolves affected windows without a full scan.
  const documentIds = normalizeDocumentIds([...(record.documentIds || []), ...(record.selectorRefIds || [])]);
  if (!documentIds.length) return;
  const windowKey = queryWindowKey(record);
  await runTransaction(
    db.transaction(STORE_QUERY_WINDOW_REFS, 'readwrite'),
    (tx) => {
      const store = tx.objectStore(STORE_QUERY_WINDOW_REFS);
      for (const documentId of documentIds) {
        store.put({
          collection: record.collection,
          documentId,
          windowKey,
        });
      }
    },
  );
}

async function deleteQueryWindowRefs(db, windowKey) {
  await runTransaction(
    db.transaction(STORE_QUERY_WINDOW_REFS, 'readwrite'),
    (tx) => {
      const index = tx.objectStore(STORE_QUERY_WINDOW_REFS).index('windowKey');
      const range = globalThis.IDBKeyRange.only(windowKey);
      const request = index.openCursor(range);
      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) return;
        cursor.delete();
        cursor.continue();
      };
    },
  );
}

function runTransaction(tx, schedule) {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error || new Error('IndexedDB transaction aborted'));
    try {
      schedule(tx);
    } catch (error) {
      try { tx.abort(); } catch {}
      reject(error);
    }
  });
}
