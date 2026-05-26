// In-memory backend for the V1.5 sidecar. Used by unit tests in Node where
// IndexedDB is not available, and as a fallback for tab environments where
// IndexedDB is unavailable. Persistence semantics are session-only.

export function createMemoryMetaBackend() {
  const queryWindows = new Map();
  const documentAccess = new Map();
  const cacheStats = new Map();

  return {
    name: 'memory',
    async putQueryWindow(record) {
      const key = queryWindowKey(record);
      queryWindows.set(key, { ...record });
    },
    async getQueryWindow(key) {
      const entry = queryWindows.get(stringKey(key));
      return entry ? { ...entry } : null;
    },
    async deleteQueryWindow(key) {
      queryWindows.delete(stringKey(key));
    },
    async scanQueryWindows() {
      return Array.from(queryWindows.values(), (record) => ({ ...record }));
    },
    async putDocumentAccess(record) {
      documentAccess.set(documentAccessKey(record), { ...record });
    },
    async getDocumentAccess(collection, id) {
      const entry = documentAccess.get(`${collection}|${id}`);
      return entry ? { ...entry } : null;
    },
    async deleteDocumentAccess(collection, id) {
      documentAccess.delete(`${collection}|${id}`);
    },
    async scanDocumentAccess() {
      return Array.from(documentAccess.values(), (record) => ({ ...record }));
    },
    async putCacheStats(record) {
      cacheStats.set(record.databaseName, { ...record });
    },
    async getCacheStats(databaseName) {
      const entry = cacheStats.get(databaseName);
      return entry ? { ...entry } : null;
    },
    async clear() {
      queryWindows.clear();
      documentAccess.clear();
      cacheStats.clear();
    },
    async close() {
      // No-op for in-memory backend.
    },
  };
}

function queryWindowKey(record) {
  return [record.collection, record.queryFingerprint, record.offset, record.limit].join('|');
}

function documentAccessKey(record) {
  return `${record.collection}|${record.id}`;
}

function stringKey(key) {
  if (Array.isArray(key)) return key.join('|');
  if (typeof key === 'string') return key;
  throw new TypeError('query window key must be array or string');
}
