import { CtoxEventEmitter } from './event-target.mjs';
import { sha256Hex } from './schema.mjs';

const DB_VERSION = 2;
const DOCUMENT_STORE = 'documents';
const SCHEMA_INDEX_ENTRIES = 'schemaIndexEntries';
const OPEN_DATABASE_TIMEOUT_MS = 4000;
const REPLICATION_SCAN_MULTIPLIER = 50;
const REPLICATION_MIN_SCAN_LIMIT = 1;
const REPLICATION_MAX_SCAN_LIMIT = 5000;
const INDEX_HIGH_KEY = '\uffff';

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
    this.primaryPath = primaryPathFromSchema(schema);
    this.indexes = normalizeSchemaIndexes(schema, this.primaryPath);
    this.indexSignature = schemaIndexSignature(this.indexes);
    this.schemaIndexReady = null;
    this.queryPerformancePolicy = { rejectAllDocumentsFallback: false };
    this.queryPerformanceStats = createQueryPerformanceStats();
    this.events = new CtoxEventEmitter();
  }

  observe(listener) {
    return this.events.on('change', (event) => listener(event?.detail || event));
  }

  async upsert(doc) {
    const id = documentId(doc);
    if (!id) {
      throw new Error(`Cannot upsert ${this.name} document without primary key`);
    }
    const { success } = await this.bulkUpsert([doc]);
    return success[id] || null;
  }

  async bulkUpsert(docs, { now = Date.now(), replicationOrigin = null } = {}) {
    if (!Array.isArray(docs)) {
      throw new TypeError('bulkUpsert docs must be an array');
    }
    const tx = this.db.transaction(DOCUMENT_STORE, 'readwrite');
    const done = idbTransactionDone(tx);
    const store = tx.objectStore(DOCUMENT_STORE);
    const success = {};
    const error = [];
    let localWriteLwtFloor = null;
    if (!replicationOrigin?.role) {
      localWriteLwtFloor = await latestCollectionLwtInTransaction(store, this.name) + 1;
    }

    for (const doc of docs) {
      const id = documentId(doc);
      if (!id) {
        error.push({ document: doc, error: 'missing primary key' });
        continue;
      }
      const previous = await idbRequest(store.get([this.name, id]));
      const nextDocument = { ...(previous?.doc || {}), ...doc };
      let lwt = documentLwt(nextDocument, now);
      if (localWriteLwtFloor !== null) {
        lwt = Math.max(lwt, localWriteLwtFloor);
        localWriteLwtFloor = lwt + 1;
      }
      if (!shouldAcceptDocumentWrite(previous, lwt, replicationOrigin)) {
        if (previous?.doc) success[id] = previous.doc;
        continue;
      }
      if (replicationOrigin?.role && previous && Number(previous.lwt || 0) >= lwt) {
        lwt = Number(previous.lwt) + 1;
      }
      const stored = storedRecordForWrite({
        collection: this.name,
        id,
        doc: nextDocument,
        lwt,
        indexes: this.indexes,
        indexSignature: this.indexSignature,
        replicationOrigin,
      });
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

  async bulkWrite(rows, { now = Date.now(), replicationOrigin = null } = {}) {
    if (!Array.isArray(rows)) {
      throw new TypeError('bulkWrite rows must be an array');
    }
    const tx = this.db.transaction(DOCUMENT_STORE, 'readwrite');
    const done = idbTransactionDone(tx);
    const store = tx.objectStore(DOCUMENT_STORE);
    const success = {};
    const error = [];
    let localWriteLwtFloor = null;
    if (!replicationOrigin?.role) {
      localWriteLwtFloor = await latestCollectionLwtInTransaction(store, this.name) + 1;
    }

    for (const row of rows) {
      const doc = row?.document || row;
      const id = documentId(doc);
      if (!id) {
        error.push({ row, error: 'missing primary key' });
        continue;
      }
      let lwt = documentLwt(doc, now);
      if (localWriteLwtFloor !== null) {
        lwt = Math.max(lwt, localWriteLwtFloor);
        localWriteLwtFloor = lwt + 1;
      }
      const previous = await idbRequest(store.get([this.name, id]));
      if (!shouldAcceptDocumentWrite(previous, lwt, replicationOrigin)) {
        continue;
      }
      if (replicationOrigin?.role && previous && Number(previous.lwt || 0) >= lwt) {
        // Accepted master state whose payload timestamp did not advance:
        // keep the stored lwt monotonic so local checkpoint consumers
        // (change feed, LWW comparisons) never see this row move backwards.
        lwt = Number(previous.lwt) + 1;
      }
      const stored = storedRecordForWrite({
        collection: this.name,
        id,
        doc,
        lwt,
        indexes: this.indexes,
        indexSignature: this.indexSignature,
        replicationOrigin,
      });
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
    const stats = this.queryPerformanceStats;
    stats.allDocumentsCalls += 1;
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const index = tx.objectStore(DOCUMENT_STORE).index('collection');
    const range = IDBKeyRange.only(this.name);
    const documents = [];
    let rowsRead = 0;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor) return false;
      const record = cursor.value;
      rowsRead += 1;
      if (withDeleted || !record.deleted) {
        documents.push(record.doc);
      }
      return true;
    });
    await idbTransactionDone(tx);
    stats.allDocumentsRowsRead += rowsRead;
    stats.lastAllDocumentsRowsRead = rowsRead;
    return documents;
  }

  async queryDocuments(query = {}, helpers = {}) {
    const primaryIds = primaryKeyCandidateIds(query, this.primaryPath);
    if (primaryIds) {
      const byId = await this.findDocumentsById(primaryIds);
      const docs = primaryIds.map((id) => byId[id]).filter(Boolean);
      return applyQueryToDocuments(docs, query, helpers);
    }
    const schemaIndexPlan = schemaIndexQueryPlanFor(query, this.indexes);
    if (schemaIndexPlan) {
      return this.queryDocumentsBySchemaIndex(schemaIndexPlan, query, helpers);
    }
    if (canUseCollectionLwtQuery(query)) {
      return this.queryDocumentsByLwt(query, helpers);
    }
    if (canUseBoundedCollectionCursor(query)) {
      return this.queryDocumentsByCollectionCursor(query, helpers);
    }
    const fallback = this.recordAllDocumentsFallback(query);
    if (this.queryPerformancePolicy.rejectAllDocumentsFallback) {
      throw createAllDocumentsFallbackError(this.name, query, fallback);
    }
    const docs = await this.allDocuments();
    fallback.rowsRead = this.queryPerformanceStats.lastAllDocumentsRowsRead || docs.length;
    this.queryPerformanceStats.allDocumentsFallbackRowsRead += fallback.rowsRead;
    return applyQueryToDocuments(docs, query, helpers);
  }

  async queryDocumentsBySchemaIndex(plan, query = {}, helpers = {}) {
    await this.ensureSchemaIndexEntries();
    const { matchesSelector = () => true, sortDocuments = (docs) => docs } = helpers || {};
    const selector = query?.selector || {};
    const skip = Number.isFinite(query?.skip) && query.skip > 0 ? query.skip : 0;
    const limit = Number.isFinite(query?.limit) ? query.limit : Number.POSITIVE_INFINITY;
    const maxMatches = plan.canStopAtLimit && Number.isFinite(limit)
      ? skip + limit
      : Number.POSITIVE_INFINITY;
    const documents = [];
    const seen = new Set();
    for (const entryRange of plan.ranges) {
      const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
      const index = tx.objectStore(DOCUMENT_STORE).index(SCHEMA_INDEX_ENTRIES);
      const range = IDBKeyRange.bound(
        [this.name, plan.index.name, ...entryRange.lower],
        [this.name, plan.index.name, ...entryRange.upper],
        Boolean(entryRange.lowerOpen),
        Boolean(entryRange.upperOpen),
      );
      await iterateCursor(index.openCursor(range, plan.direction), (cursor) => {
        if (!cursor || documents.length >= maxMatches) return false;
        const record = cursor.value;
        if (!record || seen.has(record.id)) return true;
        seen.add(record.id);
        if (!record.deleted && matchesSelector(record.doc, selector)) {
          documents.push(record.doc);
        }
        return documents.length < maxMatches;
      });
      await idbTransactionDone(tx);
      if (documents.length >= maxMatches) break;
    }
    let sorted = plan.sortCovered ? documents : sortDocuments(documents, query?.sort || []);
    if (skip > 0) sorted = sorted.slice(skip);
    if (Number.isFinite(limit)) sorted = sorted.slice(0, limit);
    return sorted;
  }

  async queryDocumentsByCollectionCursor(query = {}, helpers = {}) {
    const { matchesSelector = () => true } = helpers || {};
    const selector = query?.selector || {};
    const skip = Number.isFinite(query?.skip) && query.skip > 0 ? query.skip : 0;
    const limit = Number.isFinite(query?.limit) ? query.limit : Number.POSITIVE_INFINITY;
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const index = tx.objectStore(DOCUMENT_STORE).index('collection');
    const range = IDBKeyRange.only(this.name);
    const documents = [];
    let skipped = 0;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor || documents.length >= limit) return false;
      const record = cursor.value;
      if (!record.deleted && matchesSelector(record.doc, selector)) {
        if (skipped < skip) {
          skipped += 1;
        } else {
          documents.push(record.doc);
        }
      }
      return documents.length < limit;
    });
    await idbTransactionDone(tx);
    return documents;
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

  async countDocuments(query = {}, helpers = {}) {
    const primaryIds = primaryKeyCandidateIds(query, this.primaryPath);
    if (primaryIds) {
      const byId = await this.findDocumentsById(primaryIds);
      const docs = primaryIds.map((id) => byId[id]).filter(Boolean);
      return applyQueryToDocuments(docs, query, helpers).length;
    }
    const schemaIndexPlan = schemaIndexQueryPlanFor(query, this.indexes);
    if (schemaIndexPlan) {
      return (await this.queryDocumentsBySchemaIndex(schemaIndexPlan, query, helpers)).length;
    }
    if (canUseCollectionLwtQuery(query)) {
      return (await this.queryDocumentsByLwt(query, helpers)).length;
    }
    const { matchesSelector = () => true } = helpers || {};
    const skip = Number.isFinite(query?.skip) && query.skip > 0 ? query.skip : 0;
    const limit = Number.isFinite(query?.limit) ? query.limit : Number.POSITIVE_INFINITY;
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const index = tx.objectStore(DOCUMENT_STORE).index('collection');
    const range = IDBKeyRange.only(this.name);
    let skipped = 0;
    let count = 0;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor || count >= limit) return false;
      const record = cursor.value;
      if (!record.deleted && matchesSelector(record.doc, query?.selector || {})) {
        if (skipped < skip) {
          skipped += 1;
        } else {
          count += 1;
        }
      }
      return count < limit;
    });
    await idbTransactionDone(tx);
    return count;
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
    return {
      documents,
      checkpoint: nextCheckpoint,
      scanned,
      scanLimit,
      scanLimitReached: scanned >= scanLimit,
    };
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
    const schemaIndexPlan = schemaIndexQueryPlanFor(query, this.indexes);
    const primaryIds = primaryKeyCandidateIds(query, this.primaryPath);
    const strategy = primaryIds
      ? 'primary-key'
      : schemaIndexPlan
        ? 'schema-index'
        : canUseCollectionLwtQuery(query)
          ? 'collection-lwt'
          : canUseBoundedCollectionCursor(query)
            ? 'bounded-collection'
            : 'all-documents';
    return {
      collection: this.name,
      selectorFields,
      sortFields,
      selectedIndex: schemaIndexPlan?.index || null,
      candidateIndex: schemaIndexPlan ? null : selectBestIndex(this.indexes, selectorFields, sortFields),
      strategy,
      indexed: strategy === 'primary-key' || strategy === 'schema-index' || strategy === 'collection-lwt',
      schemaIndexed: Boolean(schemaIndexPlan),
      sortCovered: Boolean(schemaIndexPlan?.sortCovered),
      allDocumentsFallback: strategy === 'all-documents',
    };
  }

  setQueryPerformancePolicy(policy = {}) {
    this.queryPerformancePolicy = {
      ...this.queryPerformancePolicy,
      rejectAllDocumentsFallback: policy.rejectAllDocumentsFallback === true,
    };
  }

  getQueryPerformanceStats() {
    return cloneJson(this.queryPerformanceStats);
  }

  resetQueryPerformanceStats() {
    this.queryPerformanceStats = createQueryPerformanceStats();
  }

  recordAllDocumentsFallback(query = {}) {
    const plan = this.queryPlanFor(query);
    const fingerprint = queryFingerprintForStats(query);
    const fallback = {
      at: Date.now(),
      collection: this.name,
      fingerprint,
      selectorFields: plan.selectorFields || [],
      sortFields: plan.sortFields || [],
      limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : null,
      skip: Number.isFinite(Number(query?.skip)) ? Number(query.skip) : 0,
      rowsRead: 0,
    };
    this.queryPerformanceStats.allDocumentsFallbackCalls += 1;
    this.queryPerformanceStats.lastAllDocumentsFallback = fallback;
    return fallback;
  }

  ensureSchemaIndexEntries() {
    if (!this.indexes.length) return Promise.resolve(0);
    if (!this.schemaIndexReady) {
      this.schemaIndexReady = this.rebuildMissingSchemaIndexEntries();
    }
    return this.schemaIndexReady;
  }

  async rebuildMissingSchemaIndexEntries() {
    const tx = this.db.transaction(DOCUMENT_STORE, 'readwrite');
    const index = tx.objectStore(DOCUMENT_STORE).index('collection');
    const range = IDBKeyRange.only(this.name);
    let updated = 0;
    await iterateCursor(index.openCursor(range), (cursor) => {
      if (!cursor) return false;
      const record = cursor.value;
      if (record?.schemaIndexSignature !== this.indexSignature) {
        const next = {
          ...record,
          indexValues: indexValuesFor(this.indexes, record.doc || {}),
          schemaIndexSignature: this.indexSignature,
          schemaIndexEntries: schemaIndexEntriesFor(this.indexes, record.doc || {}, record.id, this.name),
        };
        cursor.update(next);
        updated += 1;
      }
      return true;
    });
    await idbTransactionDone(tx);
    return updated;
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
      let store = null;
      if (!db.objectStoreNames.contains(DOCUMENT_STORE)) {
        store = db.createObjectStore(DOCUMENT_STORE, { keyPath: ['collection', 'id'] });
        store.createIndex('collection', 'collection', { unique: false });
        store.createIndex('collectionLwtId', ['collection', 'lwt', 'id'], { unique: false });
      } else {
        store = request.transaction.objectStore(DOCUMENT_STORE);
      }
      if (store && !store.indexNames.contains(SCHEMA_INDEX_ENTRIES)) {
        store.createIndex(SCHEMA_INDEX_ENTRIES, SCHEMA_INDEX_ENTRIES, {
          unique: false,
          multiEntry: true,
        });
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

function storedRecordForWrite({ collection, id, doc, lwt, indexes, indexSignature, replicationOrigin = null }) {
  return {
    collection,
    id,
    lwt,
    deleted: Boolean(doc._deleted),
    indexValues: indexValuesFor(indexes, doc),
    schemaIndexSignature: indexSignature,
    schemaIndexEntries: schemaIndexEntriesFor(indexes, doc, id, collection),
    doc: normalizeDocument(doc, lwt, replicationOrigin),
  };
}

function shouldAcceptDocumentWrite(existingRecord, incomingLwt, replicationOrigin = null) {
  if (!existingRecord) return true;
  const existingLwt = Number(existingRecord.lwt || existingRecord.doc?._meta?.lwt || 0);
  const nextLwt = Number(incomingLwt || 0);
  if (!Number.isFinite(existingLwt) || !Number.isFinite(nextLwt)) return true;
  if (replicationOrigin?.role) {
    // Replication writes carry the MASTER's authoritative state for this id
    // (master checkpoint iteration only moves forward). The app-level
    // `updated_at_ms` lwt heuristic must not veto them: master rows arrive
    // WITHOUT `_meta.lwt` (keep_meta=false on the wire), so a master change
    // whose payload timestamp did not advance was silently dropped here
    // while the pull checkpoint advanced past it — a permanent divergence
    // (rxdb-soak file-chunk-stale-generation mode). Only an unsynced LOCAL
    // write (no ctoxReplicationOrigin marker) with a newer lwt may win,
    // until its own push round-trips through the master.
    const existingIsLocalWrite = !existingRecord.doc?._meta?.ctoxReplicationOrigin;
    if (!existingIsLocalWrite) return true;
  }
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

async function latestCollectionLwtInTransaction(store, collection) {
  const index = store.index('collectionLwtId');
  const range = IDBKeyRange.bound(
    [collection, 0, ''],
    [collection, Number.MAX_SAFE_INTEGER, '\uffff'],
    false,
    false,
  );
  const record = await firstCursorValue(index.openCursor(range, 'prev'));
  const latest = Number(record?.lwt || 0);
  return Number.isFinite(latest) && latest > 0 ? latest : 0;
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

function primaryPathFromSchema(schema = {}) {
  const primary = schema?.primaryKey;
  if (typeof primary === 'string') return primary;
  if (primary?.key) return primary.key;
  return 'id';
}

function normalizeSchemaIndexes(schema = {}, primaryPath = primaryPathFromSchema(schema)) {
  const indexes = Array.isArray(schema?.indexes) ? schema.indexes : [];
  const normalized = indexes.map((index) => normalizeSchemaIndexFields(index, primaryPath));
  if (!normalized.length) normalized.push(['_deleted', primaryPath]);
  normalized.push(['_meta.lwt', primaryPath]);
  if (Array.isArray(schema?.internalIndexes)) {
    for (const index of schema.internalIndexes) {
      normalized.push(normalizeSchemaIndexFields(index, primaryPath, { preservePrefix: true }));
    }
  }
  const seen = new Set();
  return normalized.map((fields, position) => {
    const key = fields.join(',');
    if (seen.has(key)) return null;
    seen.add(key);
    return { name: `idx_${position}_${fields.join('_')}`, fields };
  }).filter(Boolean);
}

function normalizeSchemaIndexFields(index, primaryPath, { preservePrefix = false } = {}) {
  const fields = Array.isArray(index) ? index : [index];
  const normalizedFields = fields
    .map((field) => String(field || '').trim())
    .filter(Boolean);
  if (!normalizedFields.length) return ['_deleted', primaryPath];
  const next = normalizedFields.slice();
  if (!next.includes(primaryPath)) next.push(primaryPath);
  if (!preservePrefix && next[0] !== '_deleted') next.unshift('_deleted');
  return next;
}

function primaryKeyCandidateIds(query = {}, primaryPath = 'id') {
  const selector = query?.selector || {};
  for (const field of ['id', '_id', primaryPath].filter(Boolean)) {
    if (!Object.prototype.hasOwnProperty.call(selector, field)) continue;
    const value = selector[field];
    if (value == null) return [];
    if (typeof value === 'string' || typeof value === 'number') {
      return [String(value)];
    }
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      if ('$eq' in value && value.$eq != null) return [String(value.$eq)];
      if ('$in' in value && Array.isArray(value.$in)) {
        return [...new Set(value.$in.filter((id) => id != null).map((id) => String(id)))];
      }
    }
    return null;
  }
  return null;
}

function indexValuesFor(indexes, doc) {
  const values = {};
  for (const index of indexes || []) {
    values[index.name] = index.fields.map((field) => valueAtPath(doc, field));
  }
  return values;
}

function schemaIndexEntriesFor(indexes, doc, id, collection) {
  const entries = [];
  for (const index of indexes || []) {
    const components = [];
    let usable = true;
    for (const field of index.fields) {
      const encoded = encodeIndexValue(valueAtPath(doc, field));
      if (!encoded) {
        usable = false;
        break;
      }
      components.push(...encoded);
    }
    if (usable) {
      entries.push([collection, index.name, ...components, String(id || documentId(doc))]);
    }
  }
  return entries;
}

function schemaIndexSignature(indexes = []) {
  return indexes.map((index) => `${index.name}:${index.fields.join(',')}`).join('|');
}

function selectBestIndex(indexes, selectorFields = [], sortFields = []) {
  const wanted = [...selectorFields, ...sortFields].filter(Boolean);
  if (!wanted.length) return null;
  let best = null;
  let bestScore = 0;
  for (const index of indexes || []) {
    let score = 0;
    const fields = index.fields[0] === '_deleted' ? index.fields.slice(1) : index.fields;
    for (const field of fields) {
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

function schemaIndexQueryPlanFor(query = {}, indexes = []) {
  const selector = query?.selector || {};
  if (Object.keys(selector).some((field) => field.startsWith('$'))) return null;
  const sortEntries = normalizeSortEntries(query?.sort);
  let best = null;
  let bestScore = 0;
  for (const index of indexes || []) {
    const plan = schemaIndexPlanForIndex(index, selector, sortEntries, query);
    if (!plan) continue;
    const score = (plan.constrainedFields * 10)
      + (plan.sortCovered ? 4 : 0)
      + (plan.canStopAtLimit ? 2 : 0)
      - Math.max(0, plan.ranges.length - 1);
    if (score > bestScore) {
      best = plan;
      bestScore = score;
    }
  }
  return best;
}

function schemaIndexPlanForIndex(index, selector, sortEntries, query) {
  if (!index?.fields?.length) return null;
  let ranges = [{ lower: [], upper: [], upperComplete: false }];
  let constrainedFields = 0;
  let lastEqualityFieldIndex = -1;
  let rangeFieldIndex = -1;
  let stoppedAtFieldIndex = index.fields.length;

  for (let fieldIndex = 0; fieldIndex < index.fields.length; fieldIndex += 1) {
    const field = index.fields[fieldIndex];
    const constraint = field === '_deleted'
      ? { kind: 'eq', values: [false], implicit: true }
      : selectorConstraintFor(selector, field);
    if (constraint.kind === 'none') {
      stoppedAtFieldIndex = fieldIndex;
      break;
    }
    if (constraint.kind === 'unsupported') return null;
    if (constraint.kind === 'eq') {
      const encodedValues = constraint.values
        .map((value) => encodeIndexValue(value))
        .filter(Boolean);
      if (!encodedValues.length || encodedValues.length !== constraint.values.length) return null;
      if (encodedValues.length > 32) return null;
      ranges = ranges.flatMap((range) => encodedValues.map((encoded) => ({
        lower: [...range.lower, ...encoded],
        upper: [...range.upper, ...encoded],
        upperComplete: false,
      })));
      if (!constraint.implicit) constrainedFields += 1;
      lastEqualityFieldIndex = fieldIndex;
      continue;
    }
    if (constraint.kind === 'range') {
      const lowerEncoded = constraint.lower !== undefined ? encodeIndexValue(constraint.lower) : null;
      const upperEncoded = constraint.upper !== undefined ? encodeIndexValue(constraint.upper) : null;
      if ((constraint.lower !== undefined && !lowerEncoded) || (constraint.upper !== undefined && !upperEncoded)) {
        return null;
      }
      ranges = ranges.map((range) => ({
        lower: lowerEncoded
          ? [...range.lower, ...lowerEncoded, ...(constraint.lowerOpen ? [INDEX_HIGH_KEY] : [])]
          : [...range.lower],
        upper: upperEncoded
          ? [...range.upper, ...upperEncoded, ...(constraint.upperOpen ? [] : [INDEX_HIGH_KEY])]
          : [...range.upper, INDEX_HIGH_KEY],
        upperComplete: true,
      }));
      constrainedFields += 1;
      rangeFieldIndex = fieldIndex;
      stoppedAtFieldIndex = fieldIndex + 1;
      break;
    }
  }

  const hasSelectorConstraint = constrainedFields > 0;
  const orderStart = Math.max(
    0,
    rangeFieldIndex >= 0 ? rangeFieldIndex : lastEqualityFieldIndex + 1,
  );
  const sortCovered = isSortCoveredByIndex(index.fields, orderStart, sortEntries);
  const hasSortOnlyPlan = !hasSelectorConstraint
    && sortEntries.length > 0
    && sortCovered
    && Number.isFinite(query?.limit);
  if (!hasSelectorConstraint && !hasSortOnlyPlan) return null;
  if (sortEntries.length && !sortCovered && !hasSelectorConstraint) return null;

  ranges = ranges.map((range) => ({
    lower: range.lower,
    upper: range.upperComplete || range.upper.length > range.lower.length
      ? range.upper
      : [...range.upper, INDEX_HIGH_KEY],
  }));
  const direction = sortCovered && sortEntries[0]?.direction === 'desc' ? 'prev' : 'next';
  return {
    index,
    ranges,
    direction,
    sortCovered,
    canStopAtLimit: sortCovered,
    constrainedFields,
    stoppedAtFieldIndex,
  };
}

function selectorConstraintFor(selector, field) {
  if (!Object.prototype.hasOwnProperty.call(selector, field)) return { kind: 'none' };
  const value = selector[field];
  if (isIndexComparableValue(value)) return { kind: 'eq', values: [value] };
  if (!value || typeof value !== 'object' || Array.isArray(value)) return { kind: 'unsupported' };
  const keys = Object.keys(value);
  if (keys.length === 1 && keys[0] === '$eq' && isIndexComparableValue(value.$eq)) {
    return { kind: 'eq', values: [value.$eq] };
  }
  if (keys.length === 1 && keys[0] === '$in' && Array.isArray(value.$in)) {
    const values = [...new Set(value.$in.filter(isIndexComparableValue))];
    return values.length === value.$in.length ? { kind: 'eq', values } : { kind: 'unsupported' };
  }
  const rangeKeys = new Set(['$gt', '$gte', '$lt', '$lte']);
  if (keys.length && keys.every((key) => rangeKeys.has(key))) {
    const lower = '$gt' in value ? value.$gt : value.$gte;
    const upper = '$lt' in value ? value.$lt : value.$lte;
    if ((lower !== undefined && !isIndexComparableValue(lower))
      || (upper !== undefined && !isIndexComparableValue(upper))) {
      return { kind: 'unsupported' };
    }
    return {
      kind: 'range',
      lower,
      upper,
      lowerOpen: '$gt' in value,
      upperOpen: '$lt' in value,
    };
  }
  return { kind: 'unsupported' };
}

function isSortCoveredByIndex(indexFields, orderStart, sortEntries) {
  if (!sortEntries.length) return true;
  const directions = new Set(sortEntries.map((entry) => entry.direction));
  if (directions.size > 1) return false;
  const orderedFields = indexFields.slice(orderStart).filter((field) => field !== '_deleted');
  return sortEntries.every((entry, offset) => orderedFields[offset] === entry.field);
}

function normalizeSortEntries(sort = []) {
  if (!sort) return [];
  const entries = typeof sort === 'string' ? [sort] : Array.isArray(sort) ? sort : [];
  return entries.map((entry) => {
    if (typeof entry === 'string') return { field: entry, direction: 'asc' };
    const [field, rawDirection] = Object.entries(entry || {})[0] || [];
    if (!field) return null;
    const direction = rawDirection === -1 || String(rawDirection).toLowerCase() === 'desc' ? 'desc' : 'asc';
    return { field, direction };
  }).filter(Boolean);
}

function encodeIndexValue(value) {
  if (typeof value === 'boolean') return ['b', value ? 1 : 0];
  if (typeof value === 'number' && Number.isFinite(value)) return ['n', value];
  if (typeof value === 'string') return ['s', value];
  if (value instanceof Date && Number.isFinite(value.getTime())) return ['n', value.getTime()];
  return null;
}

function isIndexComparableValue(value) {
  return Boolean(encodeIndexValue(value));
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

function canUseBoundedCollectionCursor(query = {}) {
  if (!Number.isFinite(query?.limit)) return false;
  return normalizeSortFields(query?.sort).length === 0;
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

function createQueryPerformanceStats() {
  return {
    allDocumentsCalls: 0,
    allDocumentsRowsRead: 0,
    allDocumentsFallbackCalls: 0,
    allDocumentsFallbackRowsRead: 0,
    lastAllDocumentsRowsRead: 0,
    lastAllDocumentsFallback: null,
  };
}

function createAllDocumentsFallbackError(collection, query, fallback) {
  const error = new Error(`IndexedDB query for ${collection} would use allDocuments() fallback.`);
  error.name = 'CtoxIndexedDbQueryPlanError';
  error.code = 'CTOX_INDEXEDDB_ALL_DOCUMENTS_FALLBACK';
  error.collection = collection;
  error.query = query;
  error.fallback = fallback;
  return error;
}

function queryFingerprintForStats(query = {}) {
  try {
    return JSON.stringify({
      selector: query?.selector || {},
      sort: query?.sort || [],
      skip: Number.isFinite(Number(query?.skip)) ? Number(query.skip) : 0,
      limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : null,
    });
  } catch {
    return String(Date.now());
  }
}

function cloneJson(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value));
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
  createAllDocumentsFallbackError,
  createQueryPerformanceStats,
  documentMatchesReplicationOrigin,
  indexValuesFor,
  normalizeDocument,
  normalizeSchemaIndexes,
  canUseBoundedCollectionCursor,
  encodeIndexValue,
  primaryKeyCandidateIds,
  replicationScanLimit,
  schemaIndexEntriesFor,
  schemaIndexQueryPlanFor,
  selectBestIndex,
  shouldAcceptDocumentWrite,
};
