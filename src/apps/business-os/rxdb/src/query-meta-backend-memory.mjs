// In-memory backend for the V1.5 sidecar. Used by unit tests in Node where
// IndexedDB is not available, and as a fallback for tab environments where
// IndexedDB is unavailable. Persistence semantics are session-only.

export function createMemoryMetaBackend() {
  const queryWindows = new Map();
  const queryWindowRefsByDocument = new Map();
  const queryWindowRefsByWindow = new Map();
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
      const normalizedKey = stringKey(key);
      queryWindows.delete(normalizedKey);
      deleteQueryWindowRefs(normalizedKey);
    },
    async scanQueryWindows() {
      return Array.from(queryWindows.values(), (record) => ({ ...record }));
    },
    async replaceQueryWindowDocumentRefs(record) {
      const windowKey = queryWindowKey(record);
      deleteQueryWindowRefs(windowKey);
      const documentKeys = new Set();
      // SYNC-52: selector ref ids ($nonsimple / $field|<path>) share the same
      // index so the change-invalidation path avoids a full window scan.
      for (const id of normalizeDocumentIds([...(record.documentIds || []), ...(record.selectorRefIds || [])])) {
        const documentKey = `${record.collection}|${id}`;
        documentKeys.add(documentKey);
        const refs = queryWindowRefsByDocument.get(documentKey) || new Set();
        refs.add(windowKey);
        queryWindowRefsByDocument.set(documentKey, refs);
      }
      queryWindowRefsByWindow.set(windowKey, documentKeys);
    },
    async getQueryWindowKeysByDocumentIds(collection, ids) {
      const keys = new Set();
      for (const id of normalizeDocumentIds(ids)) {
        const refs = queryWindowRefsByDocument.get(`${collection}|${id}`);
        if (!refs) continue;
        for (const key of refs) keys.add(key);
      }
      return Array.from(keys);
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
      queryWindowRefsByDocument.clear();
      queryWindowRefsByWindow.clear();
      documentAccess.clear();
      cacheStats.clear();
    },
    async close() {
      // No-op for in-memory backend.
    },
  };

  function deleteQueryWindowRefs(windowKey) {
    const documentKeys = queryWindowRefsByWindow.get(windowKey);
    if (!documentKeys) return;
    for (const documentKey of documentKeys) {
      const refs = queryWindowRefsByDocument.get(documentKey);
      if (!refs) continue;
      refs.delete(windowKey);
      if (!refs.size) queryWindowRefsByDocument.delete(documentKey);
    }
    queryWindowRefsByWindow.delete(windowKey);
  }
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

function normalizeDocumentIds(ids) {
  if (!Array.isArray(ids)) return [];
  return Array.from(new Set(ids.map((id) => String(id || '')).filter(Boolean)));
}
