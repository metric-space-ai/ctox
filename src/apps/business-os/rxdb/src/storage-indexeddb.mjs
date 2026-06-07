import { CtoxEventEmitter } from './event-target.mjs';
import { sha256Hex } from './schema.mjs';

const DB_VERSION = 1;
const DOCUMENT_STORE = 'documents';
const OPEN_DATABASE_TIMEOUT_MS = 4000;
const REPLICATION_SCAN_MULTIPLIER = 50;
const REPLICATION_MIN_SCAN_LIMIT = 500;
const REPLICATION_MAX_SCAN_LIMIT = 5000;

export async function openCtoxIndexedDbStorage({ databaseName = 'ctox_business_os_js_v1' } = {}) {
  if (!globalThis.indexedDB) {
    throw new Error('indexedDB is required for ctox-rxdb-js storage');
  }
  const db = await openDatabase(databaseName);
  return new CtoxIndexedDbStorage(db);
}

export class CtoxIndexedDbStorage {
  constructor(db) {
    this.db = db;
  }

  collection(name, { schema = null } = {}) {
    if (!name || typeof name !== 'string') {
      throw new TypeError('collection name must be a non-empty string');
    }
    return new CtoxIndexedDbCollection(this.db, name, { schema });
  }

  close() {
    this.db.close();
  }
}

export class CtoxIndexedDbCollection {
  constructor(db, name, { schema = null } = {}) {
    this.db = db;
    this.name = name;
    this.schema = schema || {};
    this.indexes = normalizeSchemaIndexes(schema);
    this.events = new CtoxEventEmitter();
  }

  observe(listener) {
    return this.events.on('change', listener);
  }

  async upsert(doc) {
    const id = documentId(doc);
    if (!id) {
      throw new Error(`Cannot upsert ${this.name} document without primary key`);
    }
    const previous = await this.findOne(id);
    await this.bulkWrite([{ previous, document: { ...(previous || {}), ...doc } }]);
    return this.findOne(id, { withDeleted: true });
  }

  async bulkWrite(rows, { now = Date.now(), replicationOrigin = null } = {}) {
    if (!Array.isArray(rows)) {
      throw new TypeError('bulkWrite rows must be an array');
    }
    const tx = this.db.transaction(DOCUMENT_STORE, 'readwrite');
    const done = idbTransactionDone(tx);
    const store = tx.objectStore(DOCUMENT_STORE);
    const success = {};
    const error = [];

    for (const row of rows) {
      const doc = row?.document || row;
      const id = documentId(doc);
      if (!id) {
        error.push({ row, error: 'missing primary key' });
        continue;
      }
      const lwt = documentLwt(doc, now);
      const stored = {
        collection: this.name,
        id,
        lwt,
        deleted: Boolean(doc._deleted),
        indexValues: indexValuesFor(this.indexes, doc),
        doc: normalizeDocument(doc, lwt, replicationOrigin),
      };
      const previous = await idbRequest(store.get([this.name, id]));
      if (!shouldAcceptDocumentWrite(previous, lwt)) {
        continue;
      }
      await idbRequest(store.put(stored));
      success[id] = stored.doc;
    }

    await done;
    if (Object.keys(success).length) {
      this.events.emit('change', {
        collection: this.name,
        success,
        at: now,
      });
    }
    return { success, error };
  }

  /// V1.5 eviction hook. Hard-deletes documents from the primary store
  /// (does NOT soft-delete via _deleted=true — the cache layer wants the
  /// row gone, not tombstoned). Caller is responsible for never invoking
  /// this on dirty docs; the sidecar enforces that.
  async hardDeleteByIds(ids) {
    if (!Array.isArray(ids) || !ids.length) return 0;
    const tx = this.db.transaction(DOCUMENT_STORE, 'readwrite');
    const store = tx.objectStore(DOCUMENT_STORE);
    let removed = 0;
    for (const id of ids) {
      await idbRequest(store.delete([this.name, String(id)]));
      removed += 1;
    }
    await idbTransactionDone(tx);
    return removed;
  }

  async findDocumentsById(ids, { withDeleted = false } = {}) {
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const done = idbTransactionDone(tx);
    const store = tx.objectStore(DOCUMENT_STORE);
    const result = {};
    for (const id of ids) {
      const record = await idbRequest(store.get([this.name, String(id)]));
      if (record && (withDeleted || !record.deleted)) {
        result[String(id)] = record.doc;
      }
    }
    await done;
    return result;
  }

  async findOne(id, { withDeleted = false } = {}) {
    const docs = await this.findDocumentsById([id], { withDeleted });
    return docs[String(id)] || null;
  }

  async allDocuments({ withDeleted = false } = {}) {
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const index = tx.objectStore(DOCUMENT_STORE).index('collection');
    const range = IDBKeyRange.only(this.name);
    const documents = [];
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor) return false;
      const record = cursor.value;
      if (withDeleted || !record.deleted) {
        documents.push(record.doc);
      }
      return true;
    });
    await idbTransactionDone(tx);
    return documents;
  }

  async queryDocuments(query = {}, helpers = {}) {
    if (canUseCollectionLwtQuery(query)) {
      return this.queryDocumentsByLwt(query, helpers);
    }
    const docs = await this.allDocuments();
    return applyQueryToDocuments(docs, query, helpers);
  }

  async queryDocumentsByLwt(query = {}, helpers = {}) {
    const { matchesSelector = () => true, sortDocuments = (docs) => docs } = helpers || {};
    const selector = query?.selector || {};
    const skip = Number.isFinite(query?.skip) && query.skip > 0 ? query.skip : 0;
    const limit = Number.isFinite(query?.limit) ? query.limit : Number.POSITIVE_INFINITY;
    const maxMatches = Number.isFinite(limit) ? skip + limit : Number.POSITIVE_INFINITY;
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const index = tx.objectStore(DOCUMENT_STORE).index('collectionLwtId');
    const range = IDBKeyRange.bound(
      [this.name, 0, ''],
      [this.name, Number.MAX_SAFE_INTEGER, '\uffff'],
      false,
      false,
    );
    const documents = [];
    await iterateCursor(index.openCursor(range, 'prev'), (cursor) => {
      if (!cursor) return false;
      const record = cursor.value;
      if (!record.deleted && matchesSelector(record.doc, selector)) {
        documents.push(record.doc);
      }
      return documents.length < maxMatches;
    });
    await idbTransactionDone(tx);
    let sorted = sortDocuments(documents, query?.sort || []);
    if (skip > 0) sorted = sorted.slice(skip);
    if (Number.isFinite(limit)) sorted = sorted.slice(0, limit);
    return sorted;
  }

  async getChangedDocumentsSince(checkpoint = null, limit = 100, options = {}) {
    const fromLwt = Number(checkpoint?.lwt || 0);
    const fromId = String(checkpoint?.id || '');
    const excludedOriginRole = String(options?.excludeReplicationOriginRole || '').trim();
    const scanLimit = replicationScanLimit(limit);
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const index = tx.objectStore(DOCUMENT_STORE).index('collectionLwtId');
    const range = IDBKeyRange.bound([this.name, fromLwt, fromId], [this.name, Number.MAX_SAFE_INTEGER, '\uffff'], true, false);
    const documents = [];
    let nextCheckpoint = checkpoint || null;
    let scanned = 0;

    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor || documents.length >= limit || scanned >= scanLimit) {
        return false;
      }
      scanned += 1;
      const record = cursor.value;
      nextCheckpoint = { lwt: record.lwt, id: record.id };
      if (!documentMatchesReplicationOrigin(record.doc, excludedOriginRole)) {
        documents.push(record.doc);
      }
      return true;
    });
    await idbTransactionDone(tx);
    return { documents, checkpoint: nextCheckpoint };
  }

  async replicationCheckpointStatus(schemaHash = null) {
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const index = tx.objectStore(DOCUMENT_STORE).index('collectionLwtId');
    const range = IDBKeyRange.bound([this.name, 0, ''], [this.name, Number.MAX_SAFE_INTEGER, '\uffff'], false, false);
    const record = await firstCursorValue(index.openCursor(range, 'prev'));
    await idbTransactionDone(tx);
    if (!record) {
      return {
        source: 'browser',
        state: 'advertised',
        collection: this.name,
        schemaHash,
        latestLwt: null,
        latestIdHash: null,
        epoch: `browser:${this.name}:empty`,
      };
    }
    const latestIdHash = await sha256Hex(record.id);
    return {
      source: 'browser',
      state: 'advertised',
      collection: this.name,
      schemaHash,
      latestLwt: record.lwt,
      latestIdHash,
      epoch: `browser:${this.name}:${record.lwt}:${latestIdHash.slice(0, 16)}`,
    };
  }

  schemaIndexes() {
    return this.indexes.map((index) => ({ ...index, fields: [...index.fields] }));
  }

  queryPlanFor(query = {}) {
    const selectorFields = Object.keys(query?.selector || {}).filter((field) => !field.startsWith('$'));
    const sortFields = normalizeSortFields(query?.sort);
    const selectedIndex = selectBestIndex(this.indexes, selectorFields, sortFields);
    return {
      collection: this.name,
      selectorFields,
      sortFields,
      selectedIndex,
      indexed: Boolean(selectedIndex),
    };
  }
}

function openDatabase(databaseName) {
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
      finish(reject, new Error(`IndexedDB open timed out after ${OPEN_DATABASE_TIMEOUT_MS}ms for ${databaseName}`));
    }, OPEN_DATABASE_TIMEOUT_MS);
    const request = indexedDB.open(databaseName, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(DOCUMENT_STORE)) {
        const store = db.createObjectStore(DOCUMENT_STORE, { keyPath: ['collection', 'id'] });
        store.createIndex('collection', 'collection', { unique: false });
        store.createIndex('collectionLwtId', ['collection', 'lwt', 'id'], { unique: false });
      }
    };
    request.onsuccess = () => {
      if (!finish(resolve, request.result)) {
        try { request.result?.close?.(); } catch {}
      }
    };
    request.onerror = () => finish(reject, request.error || new Error(`Failed to open IndexedDB ${databaseName}`));
    request.onblocked = () => finish(reject, new Error(`IndexedDB open blocked for ${databaseName}`));
  });
}

function documentId(doc) {
  if (!doc || typeof doc !== 'object') {
    return '';
  }
  return String(doc.id || doc._id || doc.document_id || doc.documentId || '');
}

function normalizeDocument(doc, lwt, replicationOrigin = null) {
  const normalized = { ...doc };
  const id = documentId(doc);
  if (!normalized.id) {
    normalized.id = id;
  }
  normalized._meta = { ...(normalized._meta || {}), lwt };
  if (replicationOrigin?.role) {
    normalized._meta.ctoxReplicationOrigin = sanitizeReplicationOrigin(replicationOrigin);
  } else {
    delete normalized._meta.ctoxReplicationOrigin;
  }
  normalized._deleted = Boolean(normalized._deleted);
  return normalized;
}

function shouldAcceptDocumentWrite(existingRecord, incomingLwt) {
  if (!existingRecord) return true;
  const existingLwt = Number(existingRecord.lwt || existingRecord.doc?._meta?.lwt || 0);
  const nextLwt = Number(incomingLwt || 0);
  if (!Number.isFinite(existingLwt) || !Number.isFinite(nextLwt)) return true;
  return nextLwt >= existingLwt;
}

function documentLwt(doc = {}, fallback = Date.now()) {
  const values = [
    Number(doc._meta?.lwt || 0),
    Number(doc.updated_at_ms || 0),
    Number(doc.updatedAtMs || 0),
  ].filter((value) => Number.isFinite(value) && value > 0);
  return values.length ? Math.max(...values) : Number(fallback || Date.now());
}

function sanitizeReplicationOrigin(origin) {
  return {
    role: String(origin.role || '').slice(0, 64),
    peerId: String(origin.peerId || '').slice(0, 160),
    sessionId: String(origin.sessionId || '').slice(0, 160),
    collection: String(origin.collection || '').slice(0, 160),
  };
}

function documentMatchesReplicationOrigin(doc, excludedOriginRole) {
  if (!excludedOriginRole) return false;
  const origin = doc?._meta?.ctoxReplicationOrigin;
  return origin?.role === excludedOriginRole;
}

function replicationScanLimit(limit) {
  const batchLimit = Number.isFinite(limit) && limit > 0 ? limit : 100;
  return Math.max(
    REPLICATION_MIN_SCAN_LIMIT,
    Math.min(REPLICATION_MAX_SCAN_LIMIT, Math.ceil(batchLimit * REPLICATION_SCAN_MULTIPLIER)),
  );
}

function normalizeSchemaIndexes(schema = {}) {
  const indexes = Array.isArray(schema?.indexes) ? schema.indexes : [];
  return indexes.map((index, position) => {
    const fields = Array.isArray(index) ? index : [index];
    const normalizedFields = fields
      .map((field) => String(field || '').trim())
      .filter(Boolean);
    return normalizedFields.length
      ? { name: `idx_${position}_${normalizedFields.join('_')}`, fields: normalizedFields }
      : null;
  }).filter(Boolean);
}

function indexValuesFor(indexes, doc) {
  const values = {};
  for (const index of indexes || []) {
    values[index.name] = index.fields.map((field) => valueAtPath(doc, field));
  }
  return values;
}

function selectBestIndex(indexes, selectorFields = [], sortFields = []) {
  const wanted = [...selectorFields, ...sortFields].filter(Boolean);
  if (!wanted.length) return null;
  let best = null;
  let bestScore = 0;
  for (const index of indexes || []) {
    let score = 0;
    for (const field of index.fields) {
      if (wanted.includes(field)) score += 1;
      else break;
    }
    if (score > bestScore) {
      best = index;
      bestScore = score;
    }
  }
  return best ? { ...best, fields: [...best.fields], matchedFields: bestScore } : null;
}

function canUseCollectionLwtQuery(query = {}) {
  if (!Number.isFinite(query?.limit)) return false;
  const sortFields = normalizeSortFields(query?.sort);
  if (!sortFields.length) return false;
  const firstSort = sortFields[0];
  if (!['updated_at_ms', 'updatedAtMs', '_meta.lwt'].includes(firstSort)) return false;
  const firstSortEntry = Array.isArray(query?.sort) ? query.sort[0] : null;
  const direction = typeof firstSortEntry === 'string'
    ? 'asc'
    : String(Object.values(firstSortEntry || {})[0] || '').toLowerCase();
  return ['desc', '-1'].includes(direction);
}

function applyQueryToDocuments(docs = [], query = {}, helpers = {}) {
  const { matchesSelector = () => true, sortDocuments = (items) => items } = helpers || {};
  let filtered = docs.filter((doc) => matchesSelector(doc, query?.selector || {}));
  filtered = sortDocuments(filtered, query?.sort || []);
  if (Number.isFinite(query?.skip) && query.skip > 0) {
    filtered = filtered.slice(query.skip);
  }
  if (Number.isFinite(query?.limit)) {
    filtered = filtered.slice(0, query.limit);
  }
  return filtered;
}

function normalizeSortFields(sort = []) {
  if (!Array.isArray(sort)) return typeof sort === 'string' ? [sort] : [];
  return sort.map((entry) => {
    if (typeof entry === 'string') return entry;
    return Object.keys(entry || {})[0] || '';
  }).filter(Boolean);
}

function valueAtPath(doc, path) {
  return String(path || '').split('.').reduce((value, key) => value?.[key], doc);
}

function idbRequest(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function idbTransactionDone(tx) {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onabort = () => reject(tx.error || new Error('IndexedDB transaction aborted'));
    tx.onerror = () => reject(tx.error || new Error('IndexedDB transaction failed'));
  });
}

function iterateCursor(request, visitor) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => {
      const cursor = request.result;
      if (!cursor) {
        resolve();
        return;
      }
      const shouldContinue = visitor(cursor);
      if (shouldContinue === false) {
        resolve();
        return;
      }
      cursor.continue();
    };
    request.onerror = () => reject(request.error);
  });
}

function firstCursorValue(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result?.value || null);
    request.onerror = () => reject(request.error);
  });
}

export const ctoxIndexedDbStorageTestInternals = {
  documentMatchesReplicationOrigin,
  indexValuesFor,
  normalizeDocument,
  normalizeSchemaIndexes,
  replicationScanLimit,
  selectBestIndex,
  shouldAcceptDocumentWrite,
};
