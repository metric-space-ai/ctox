import { CtoxEventEmitter } from './event-target.mjs';
import { schemaHash, sha256Hex } from './schema.mjs';
import { normalizeConflictStrategy, threeWayMergeDocuments } from './conflict-merge.mjs';
import { formatHybridLogicalClock, nextHybridLogicalClock } from './hybrid-logical-clock.mjs';
import { openRecoveryJournal } from './recovery-journal.mjs';
import { recoverQueryMetaQuota } from './query-meta-storage.mjs';

const DB_VERSION = 3;
const DOCUMENT_STORE = 'documents';
const SCHEMA_INDEX_ENTRIES = 'schemaIndexEntries';
const PUSHABLE_LWT_INDEX = 'collectionPushableLwtId';
const OPEN_DATABASE_TIMEOUT_MS = 4000;
const REPLICATION_SCAN_MULTIPLIER = 50;
const REPLICATION_MIN_SCAN_LIMIT = 1;
const REPLICATION_MAX_SCAN_LIMIT = 5000;
const INDEX_HIGH_KEY = '\uffff';
const unsyncedCountScheduled = new WeakSet();

export async function openCtoxIndexedDbStorage({ databaseName = 'ctox_business_os_js_v1' } = {}) {
  if (!globalThis.indexedDB) {
    throw new Error('indexedDB is required for ctox-rxdb-js storage');
  }
  const db = await openDatabase(databaseName);
  const quotaCoordinator = {
    recover: (context = {}) => recoverQueryMetaQuota(databaseName, context),
  };
  const recoveryJournal = await openRecoveryJournal({
    databaseName,
    instanceId: databaseName,
    quotaCoordinator,
  });
  return new CtoxIndexedDbStorage(db, { recoveryJournal, quotaCoordinator });
}

export class CtoxIndexedDbStorage {
  constructor(db, { recoveryJournal = null, quotaCoordinator = null } = {}) {
    this.db = db;
    this.recoveryJournal = recoveryJournal;
    this.quotaCoordinator = quotaCoordinator;
  }

  collection(name, { schema = null, conflictStrategy = 'lww' } = {}) {
    if (!name || typeof name !== 'string') {
      throw new TypeError('collection name must be a non-empty string');
    }
    return new CtoxIndexedDbCollection(this.db, name, {
      schema,
      conflictStrategy,
      recoveryJournal: this.recoveryJournal,
      quotaCoordinator: this.quotaCoordinator,
    });
  }

  async unsyncedWriteSummary() {
    return countUnsyncedWrites(this.db);
  }

  close() {
    this.recoveryJournal?.close?.();
    this.db.close();
  }
}

export class CtoxIndexedDbCollection {
  constructor(db, name, {
    schema = null,
    conflictStrategy = 'lww',
    recoveryJournal = null,
    quotaCoordinator = null,
  } = {}) {
    this.db = db;
    this.name = name;
    this.schema = schema || {};
    // 'lww' (default) or 'field-merge' (see conflict-merge.mjs). Declared as
    // a sibling of `schema` in the collection definition so schema hashes
    // stay untouched.
    this.conflictStrategy = normalizeConflictStrategy(conflictStrategy);
    this.primaryPath = primaryPathFromSchema(schema);
    this.indexes = normalizeSchemaIndexes(schema, this.primaryPath);
    this.indexSignature = schemaIndexSignature(this.indexes);
    this.schemaIndexReady = null;
    this.queryPerformancePolicy = { rejectAllDocumentsFallback: false };
    this.queryPerformanceStats = createQueryPerformanceStats();
    // OS-C4: merge observability for field-merge collections. `pullFieldMerges`
    // counts replication pulls that merged over an unsynced local row;
    // `pushConflictMerges` counts masterWrite conflict retries that absorbed
    // master state (incremented by the replication layer). Surfaced into the
    // sync diagnostics per collection.
    this.mergeStats = { pullFieldMerges: 0, pushConflictMerges: 0 };
    this.events = new CtoxEventEmitter();
    this.recoveryJournal = recoveryJournal;
    this.quotaCoordinator = quotaCoordinator;
    this.recoverySchemaHash = '';
    this.recoveryReady = null;
    this.externalChangeListener = (event) => {
      const detail = event?.detail || {};
      if (detail.databaseName !== this.db.name || detail.collection !== this.name) return;
      this.events.emit('change', {
        collection: this.name,
        external: true,
        ids: Array.isArray(detail.ids) ? detail.ids : [],
        at: Date.now(),
      });
    };
    globalThis.addEventListener?.('ctox-rxdb-external-change', this.externalChangeListener);
  }

  close() {
    globalThis.removeEventListener?.('ctox-rxdb-external-change', this.externalChangeListener);
  }

  async initializeRecovery() {
    if (!this.recoveryJournal) return;
    if (!this.recoveryReady) {
      this.recoveryReady = (async () => {
        this.recoverySchemaHash = await schemaHash(this.schema || {}, this.name);
        this.recoveryJournal.registerCollection(this.name, {
          schemaHash: this.recoverySchemaHash,
          applyBatch: (batch) => this.runWithQuotaRecovery(
            () => batch.operation === 'upsert'
              ? this._bulkUpsertOnce(batch.rows || [], {})
              : this._bulkWriteOnce(batch.rows || [], { baseById: batch.baseById || null }),
            { source: 'recovery-replay' },
          ),
          // A user resolution is a NEW pushable local write, not recovery
          // replay. Route it through the public path so it receives a fresh
          // HLC and durable WAL entry before the primary row changes.
          resolveConflict: (batch) => this.bulkWrite(batch.rows || [], {
            baseById: batch.baseById || null,
          }),
        });
        await this.acknowledgePersistedMasterRecovery();
        await this.recoveryJournal.replayRegisteredCollections(this.name);
      })();
    }
    return this.recoveryReady;
  }

  async acknowledgePersistedMasterRecovery() {
    // This runs on the foreground path before the collection's first write.
    // Use the compound state+collection index; scanning every pending batch
    // for every collection can stall command insertion on mature profiles.
    const batches = await this.recoveryJournal?.listBatches?.('pending', this.name) || [];
    const ids = [...new Set(batches
      .filter((batch) => batch.collection === this.name)
      .flatMap((batch) => batch.documentIds || []))];
    if (!ids.length) return;
    const documents = {};
    for (const id of ids) {
      const record = await this.getStoredRecord(id);
      if (record?.replicationOriginRole && record.doc) documents[id] = record.doc;
    }
    if (Object.keys(documents).length) {
      await this.recoveryJournal.markMasterAcknowledged(this.name, documents);
    }
  }

  observe(listener) {
    return this.events.on('change', (event) => listener(event?.detail || event));
  }

  // Raw stored record (doc + base + replication flags) for one id. Used by
  // the replication push path to fetch the merge base on masterWrite
  // conflicts; not part of the query surface.
  async getStoredRecord(id) {
    if (!id) return null;
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const store = tx.objectStore(DOCUMENT_STORE);
    const record = await idbRequest(store.get([this.name, id]));
    return record || null;
  }

  // Field-merge + merge-base tracking for one incoming write that already
  // passed `shouldAcceptDocumentWrite`. Decides what actually gets stored:
  //
  //   - LOCAL write on a field-merge collection: carry the merge base along
  //     (the last master-confirmed doc, surviving consecutive local writes).
  //   - Replication write over an UNSYNCED LOCAL row on a field-merge
  //     collection: three-way merge. If local field changes survive, the
  //     result is stored as a LOCAL (pushable) write — deliberately WITHOUT
  //     the replication-origin stamp, because it still carries state the
  //     master has not seen — with the incoming master doc as the new base.
  //   - Everything else: unchanged pass-through (whole-doc LWW semantics).
  resolveIncomingWrite({ previous, doc, lwt, replicationOrigin, explicitBase }) {
    const mergeEnabled = this.conflictStrategy === 'field-merge';
    if (!replicationOrigin?.role) {
      // OS-C4: the push-conflict repair path already absorbed the master's
      // row into `doc` and passes that row as the explicit new base — using
      // the stale stored base there would re-win absorbed master fields on
      // the next merge round.
      const base = explicitBase !== undefined
        ? (mergeEnabled ? explicitBase : undefined)
        : (mergeEnabled && previous
          ? (previous.replicationOriginRole ? previous.doc : previous.base)
          : undefined);
      return { doc, lwt, replicationOrigin, base };
    }
    const existingIsLocalWrite = Boolean(previous) && !previous.doc?._meta?.ctoxReplicationOrigin;
    if (!mergeEnabled || !existingIsLocalWrite) {
      return { doc, lwt, replicationOrigin, base: undefined };
    }
    if (doc?._deleted) {
      return { doc, lwt, replicationOrigin, base: undefined };
    }
    const { merged, identicalToMaster, requiresManualResolution, conflictFields } = threeWayMergeDocuments(
      previous.base,
      previous.doc,
      doc,
      { primaryPath: this.primaryPath },
    );
    if (requiresManualResolution) {
      const error = new Error(`Structured conflict requires native/manual resolution for ${this.name}: ${conflictFields.join(', ')}`);
      error.code = 'structured_conflict_requires_resolution';
      error.collection = this.name;
      error.fields = conflictFields;
      error.base = previous.base;
      error.local = previous.doc;
      error.master = doc;
      throw error;
    }
    if (identicalToMaster) {
      // No local-only change survived (e.g. the own push round-tripped):
      // store the master row normally, which clears the base and leaves the
      // push set.
      return { doc, lwt, replicationOrigin, base: undefined };
    }
    this.mergeStats.pullFieldMerges += 1;
    const mergedLwt = Math.max(Number(lwt) || 0, Number(previous.lwt) || 0) + 1;
    return { doc: merged, lwt: mergedLwt, replicationOrigin: null, base: doc };
  }

  async upsert(doc) {
    const id = documentId(doc);
    if (!id) {
      throw new Error(`Cannot upsert ${this.name} document without primary key`);
    }
    const { success } = await this.bulkUpsert([doc]);
    return success[id] || null;
  }

  async bulkUpsert(docs, {
    now = Date.now(),
    replicationOrigin = null,
    skipJournal = false,
    recoveryReplay = false,
  } = {}) {
    await this.initializeRecovery();
    const journalWrite = Boolean(this.recoveryJournal)
      && !skipJournal && !replicationOrigin?.role && Array.isArray(docs);
    const prepared = journalWrite ? await this.prepareJournalRows(docs) : { rows: docs, baseById: null };
    const writeDocs = prepared.rows;
    const validDocs = Array.isArray(writeDocs) ? writeDocs.filter((doc) => documentId(doc)) : writeDocs;
    const journalBaseById = prepared.baseById;
    const batchId = !skipJournal && !replicationOrigin?.role && Array.isArray(validDocs) && validDocs.length
      ? await this.recoveryJournal?.appendBatch({
        collection: this.name,
        schemaHash: this.recoverySchemaHash,
        primaryPath: this.primaryPath,
        operation: 'upsert',
        rows: validDocs,
        baseById: journalBaseById,
      })
      : null;
    let result;
    try {
      await this.persistDeleteUpdateConflicts(validDocs, replicationOrigin);
      result = await this.runWithQuotaRecovery(
        () => this._bulkUpsertOnce(writeDocs, { now, replicationOrigin }),
        { source: recoveryReplay ? 'recovery-replay' : 'bulk-upsert' },
      );
    } catch (error) {
      await this.persistStructuredConflict(error);
      throw error;
    }
    if (batchId) await this.recoveryJournal.commitBatch(batchId, result.success);
    if (replicationOrigin?.role) await this.recoveryJournal?.markMasterAcknowledged(this.name, result.success);
    return result;
  }

  async _bulkUpsertOnce(docs, { now = Date.now(), replicationOrigin = null } = {}) {
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

    try {
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
      if (!shouldAcceptDocumentWrite(previous, lwt, replicationOrigin, nextDocument, this.name)) {
        if (previous?.doc) success[id] = previous.doc;
        continue;
      }
      const resolved = this.resolveIncomingWrite({
        previous,
        doc: nextDocument,
        lwt,
        replicationOrigin,
      });
      if (resolved.replicationOrigin?.role && previous && Number(previous.lwt || 0) >= resolved.lwt) {
        resolved.lwt = Number(previous.lwt) + 1;
      }
      const stored = storedRecordForWrite({
        collection: this.name,
        id,
        doc: resolved.doc,
        lwt: resolved.lwt,
        indexes: this.indexes,
        indexSignature: this.indexSignature,
        replicationOrigin: resolved.replicationOrigin,
        base: resolved.base,
        previous,
      });
      await idbRequest(store.put(stored));
      success[id] = stored.doc;
    }

    await done;
    } catch (error) {
      try { tx.abort(); } catch {}
      try { await done; } catch {}
      throw error;
    }
    schedulePersistUnsyncedWriteCount(this.db);
    if (Object.keys(success).length) {
      this.events.emit('change', {
        collection: this.name,
        success,
        at: now,
      });
      dispatchStorageChange(this.db.name, this.name, success, replicationOrigin);
    }
    return { success, error };
  }

  async bulkWrite(rows, {
    now = Date.now(),
    replicationOrigin = null,
    baseById = null,
    skipJournal = false,
    recoveryReplay = false,
  } = {}) {
    await this.initializeRecovery();
    const journalWrite = Boolean(this.recoveryJournal)
      && !skipJournal && !replicationOrigin?.role && Array.isArray(rows);
    const prepared = journalWrite ? await this.prepareJournalRows(rows) : { rows, baseById: null };
    const writeRows = prepared.rows;
    const validRows = Array.isArray(writeRows)
      ? writeRows.filter((row) => documentId(row?.document || row))
      : writeRows;
    const journalBaseById = baseById || prepared.baseById;
    const batchId = !skipJournal && !replicationOrigin?.role && Array.isArray(validRows) && validRows.length
      ? await this.recoveryJournal?.appendBatch({
        collection: this.name,
        schemaHash: this.recoverySchemaHash,
        primaryPath: this.primaryPath,
        operation: 'write',
        rows: validRows,
        baseById: journalBaseById,
      })
      : null;
    let result;
    try {
      await this.persistDeleteUpdateConflicts(validRows, replicationOrigin);
      result = await this.runWithQuotaRecovery(
        () => this._bulkWriteOnce(writeRows, { now, replicationOrigin, baseById: journalBaseById }),
        { source: recoveryReplay ? 'recovery-replay' : 'bulk-write' },
      );
    } catch (error) {
      await this.persistStructuredConflict(error);
      throw error;
    }
    if (batchId) await this.recoveryJournal.commitBatch(batchId, result.success);
    if (replicationOrigin?.role) await this.recoveryJournal?.markMasterAcknowledged(this.name, result.success);
    return result;
  }

  async _bulkWriteOnce(rows, { now = Date.now(), replicationOrigin = null, baseById = null } = {}) {
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

    try {
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
      if (!shouldAcceptDocumentWrite(previous, lwt, replicationOrigin, doc, this.name)) {
        continue;
      }
      const resolved = this.resolveIncomingWrite({
        previous,
        doc,
        lwt,
        replicationOrigin,
        explicitBase: baseById && Object.prototype.hasOwnProperty.call(baseById, id)
          ? baseById[id]
          : undefined,
      });
      if (resolved.replicationOrigin?.role && previous && Number(previous.lwt || 0) >= resolved.lwt) {
        // Accepted master state whose payload timestamp did not advance:
        // keep the stored lwt monotonic so local checkpoint consumers
        // (change feed, LWW comparisons) never see this row move backwards.
        resolved.lwt = Number(previous.lwt) + 1;
      }
      const stored = storedRecordForWrite({
        collection: this.name,
        id,
        doc: resolved.doc,
        lwt: resolved.lwt,
        indexes: this.indexes,
        indexSignature: this.indexSignature,
        replicationOrigin: resolved.replicationOrigin,
        base: resolved.base,
        previous,
      });
      await idbRequest(store.put(stored));
      success[id] = stored.doc;
    }

    await done;
    } catch (error) {
      try { tx.abort(); } catch {}
      try { await done; } catch {}
      throw error;
    }
    schedulePersistUnsyncedWriteCount(this.db);
    if (Object.keys(success).length) {
      this.events.emit('change', {
        collection: this.name,
        success,
        at: now,
      });
      dispatchStorageChange(this.db.name, this.name, success, replicationOrigin);
    }
    return { success, error };
  }

  async runWithQuotaRecovery(operation, context = {}) {
    try {
      return await operation();
    } catch (error) {
      if (!isQuotaExceededError(error)) throw error;
      await this.quotaCoordinator?.recover?.(context);
      try {
        return await operation();
      } catch (retryError) {
        const quotaError = new Error('IndexedDB write failed after safe cache eviction and one retry.', { cause: retryError });
        quotaError.code = 'indexeddb_quota_exceeded';
        quotaError.retryable = true;
        throw quotaError;
      }
    }
  }

  async persistStructuredConflict(error) {
    if (error?.code !== 'structured_conflict_requires_resolution') return;
    await this.recoveryJournal?.recordConflict?.({
      code: error.code,
      collection: this.name,
      fields: error.fields || [],
      base: error.base || null,
      local: error.local || null,
      master: error.master || null,
      message: error.message || String(error),
    });
  }

  async persistDeleteUpdateConflicts(rows, replicationOrigin) {
    if (!replicationOrigin?.role || !Array.isArray(rows)) return;
    for (const row of rows) {
      const master = row?.document || row;
      if (!master?._deleted) continue;
      const id = documentId(master, this.primaryPath);
      const previous = id ? await this.getStoredRecord(id) : null;
      if (!previous || previous.replicationOriginRole || previous.doc?._deleted) continue;
      await this.recoveryJournal?.recordConflict?.({
        code: 'structured_conflict_requires_resolution',
        conflictType: 'delete_vs_update',
        collection: this.name,
        base: previous.base || null,
        local: previous.doc,
        master,
        message: 'The native tombstone is authoritative; the local update remains recoverable here.',
      });
    }
  }

  async prepareJournalRows(rows) {
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const done = idbTransactionDone(tx);
    const store = tx.objectStore(DOCUMENT_STORE);
    const bases = {};
    const prepared = [];
    const lastHlcById = new Map();
    for (const row of rows || []) {
      const document = row?.document || row;
      const id = documentId(document);
      if (!id) {
        prepared.push(row);
        continue;
      }
      const previous = await idbRequest(store.get([this.name, id]));
      if (previous && !Object.prototype.hasOwnProperty.call(bases, id)) {
        bases[id] = previous.replicationOriginRole ? previous.doc : (previous.base || previous.doc);
      }
      const nextDocument = structuredClone(document);
      const nextHlc = nextHybridLogicalClock(
        lastHlcById.get(id) || previous?.doc?._meta?.ctoxHlc || nextDocument?._meta?.ctoxHlc,
      );
      lastHlcById.set(id, nextHlc);
      nextDocument._meta = {
        ...(nextDocument._meta || {}),
        ctoxHlc: nextHlc,
      };
      prepared.push(row?.document ? { ...row, document: nextDocument } : nextDocument);
    }
    await done;
    return { rows: prepared, baseById: bases };
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
    const usePushableIndex = shouldUsePushableReplicationIndex(excludedOriginRole);
    const scanLimit = replicationScanLimit(limit);
    const tx = this.db.transaction(DOCUMENT_STORE, 'readonly');
    const index = tx.objectStore(DOCUMENT_STORE).index(usePushableIndex ? PUSHABLE_LWT_INDEX : 'collectionLwtId');
    const range = usePushableIndex
      ? IDBKeyRange.bound([this.name, 1, fromLwt, fromId], [this.name, 1, Number.MAX_SAFE_INTEGER, '\uffff'], true, false)
      : IDBKeyRange.bound([this.name, fromLwt, fromId], [this.name, Number.MAX_SAFE_INTEGER, '\uffff'], true, false);
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
      if (store && !store.indexNames.contains(PUSHABLE_LWT_INDEX)) {
        store.createIndex(PUSHABLE_LWT_INDEX, ['collection', 'pushable', 'lwt', 'id'], { unique: false });
        migrateStoredReplicationFlags(store);
      }
    };
    request.onsuccess = () => {
      const db = request.result;
      db.onversionchange = () => {
        try { db.close(); } catch {}
        globalThis.dispatchEvent?.(new CustomEvent('ctox-indexeddb-versionchange', {
          detail: { databaseName, oldVersion: db.version },
        }));
      };
      if (!finish(resolve, db)) {
        try { request.result?.close?.(); } catch {}
      }
    };
    request.onerror = () => finish(reject, request.error || new Error(`Failed to open IndexedDB ${databaseName}`));
    request.onblocked = () => finish(reject, new Error(`IndexedDB open blocked for ${databaseName}`));
  });
}

function schedulePersistUnsyncedWriteCount(db) {
  if (unsyncedCountScheduled.has(db)) return;
  unsyncedCountScheduled.add(db);
  const timer = setTimeout(async () => {
    try {
      const summary = await countUnsyncedWrites(db);
      globalThis.localStorage?.setItem?.(
        `ctox.businessOs.unsyncedWrites.${db.name}`,
        JSON.stringify({ ...summary, capturedAtMs: Date.now() }),
      );
      const recoveryKey = `ctox.businessOs.rxdbRecoveryJournal.${db.name}`;
      const recovery = JSON.parse(globalThis.localStorage?.getItem?.(recoveryKey) || 'null');
      if (recovery) {
        if (summary.total === 0) {
          globalThis.localStorage?.removeItem?.(recoveryKey);
        } else {
          globalThis.localStorage?.setItem?.(recoveryKey, JSON.stringify({
            ...recovery,
            uniqueUnsyncedWrites: summary.total,
            unsyncedByCollection: summary.byCollection,
            updatedAtMs: Date.now(),
          }));
        }
      }
    } catch {
      // Diagnostics must never turn a committed IndexedDB write into failure.
    } finally {
      unsyncedCountScheduled.delete(db);
    }
  // Keep diagnostics out of the caller's write critical path. A short
  // debounce also coalesces bursty bulk writes into one durability scan.
  }, 1_000);
  timer?.unref?.();
}

async function countUnsyncedWrites(db) {
  const tx = db.transaction(DOCUMENT_STORE, 'readonly');
  const store = tx.objectStore(DOCUMENT_STORE);
  const byCollection = {};
  let total = 0;
  await iterateCursor(store.openCursor(), (cursor) => {
    if (!cursor) return false;
    const record = cursor.value;
    if (Number(record?.pushable || 0) === 1) {
      total += 1;
      const collection = String(record.collection || 'unknown');
      byCollection[collection] = (byCollection[collection] || 0) + 1;
    }
    return true;
  });
  await idbTransactionDone(tx);
  return { total, byCollection };
}

function documentId(doc) {
  if (!doc || typeof doc !== 'object') {
    return '';
  }
  return String(doc.id || doc._id || doc.document_id || doc.documentId || '');
}

function normalizeDocument(doc, lwt, replicationOrigin = null, previous = null) {
  const normalized = { ...doc };
  const id = documentId(doc);
  if (!normalized.id) {
    normalized.id = id;
  }
  normalized._meta = { ...(normalized._meta || {}), lwt };
  if (replicationOrigin?.role) {
    normalized._meta.ctoxHlc = normalized._meta.ctoxHlc
      || formatHybridLogicalClock({ physicalMs: lwt, nodeId: 'native' });
    normalized._meta.ctoxReplicationOrigin = sanitizeReplicationOrigin(replicationOrigin);
  } else {
    const suppliedHlc = String(normalized._meta.ctoxHlc || '');
    const previousHlc = String(previous?._meta?.ctoxHlc || '');
    normalized._meta.ctoxHlc = suppliedHlc && suppliedHlc !== previousHlc
      ? suppliedHlc
      : nextHybridLogicalClock(previousHlc || suppliedHlc, {
        nowMs: Math.max(Date.now(), Number(lwt) || 0),
      });
    delete normalized._meta.ctoxReplicationOrigin;
  }
  normalized._deleted = Boolean(normalized._deleted);
  return normalized;
}

function storedRecordForWrite({ collection, id, doc, lwt, indexes, indexSignature, replicationOrigin = null, base = undefined, previous = null }) {
  const normalizedDoc = normalizeDocument(doc, lwt, replicationOrigin, previous?.doc || null);
  const replicationOriginRole = String(replicationOrigin?.role || '').slice(0, 64);
  const record = {
    collection,
    id,
    lwt,
    deleted: Boolean(doc._deleted),
    replicationOriginRole,
    pushable: replicationOriginRole ? 0 : 1,
    indexValues: indexValuesFor(indexes, doc),
    schemaIndexSignature: indexSignature,
    schemaIndexEntries: schemaIndexEntriesFor(indexes, doc, id, collection),
    doc: normalizedDoc,
  };
  // Merge base (field-merge collections only): the last master-confirmed
  // state a local edit diverged from. Lives on the record — never inside
  // `doc` — so it stays off the wire. `put` replaces the whole record, so an
  // absent base here clears any previous one.
  if (base !== undefined && base !== null) {
    record.base = base;
  }
  return record;
}

function migrateStoredReplicationFlags(store) {
  const request = store.openCursor();
  request.onsuccess = () => {
    const cursor = request.result;
    if (!cursor) return;
    const next = normalizeStoredReplicationFlags(cursor.value);
    if (next !== cursor.value) {
      cursor.update(next);
    }
    cursor.continue();
  };
}

function normalizeStoredReplicationFlags(record) {
  if (!record || typeof record !== 'object') return record;
  const role = String(record.doc?._meta?.ctoxReplicationOrigin?.role || '').slice(0, 64);
  const pushable = role ? 0 : 1;
  if (record.replicationOriginRole === role && record.pushable === pushable) {
    return record;
  }
  return {
    ...record,
    replicationOriginRole: role,
    pushable,
  };
}

function shouldAcceptDocumentWrite(
  existingRecord,
  incomingLwt,
  replicationOrigin = null,
  incomingDocument = null,
  collectionName = '',
) {
  if (!existingRecord) return true;
  const existingLwt = Number(existingRecord.lwt || existingRecord.doc?._meta?.lwt || 0);
  const nextLwt = Number(incomingLwt || 0);
  if (!Number.isFinite(existingLwt) || !Number.isFinite(nextLwt)) return true;
  if (replicationOrigin?.role) {
    // A push can race the native command consumer: the consumer's `accepted`
    // change may arrive through the live stream before an older in-flight
    // `masterChangesSince` response that still contains `pending_sync`.
    // Both are master-origin writes, so timestamp-only acceptance would let
    // the stale response regress the command forever after the pull checkpoint
    // had already advanced. Server-owned command lifecycle state is monotonic.
    if (
      collectionName === 'business_commands'
      && isStaleReplicatedBusinessCommandState(existingRecord.doc, incomingDocument)
    ) {
      return false;
    }
    // Browser and daemon clocks do not have to be identical. A browser can
    // therefore stamp its pending command a little later than the daemon
    // stamps the authoritative accepted/terminal projection. Lifecycle
    // progress must win over that skew; otherwise the pull checkpoint moves
    // past a completed command while IndexedDB remains pending forever.
    if (
      collectionName === 'business_commands'
      && isForwardReplicatedBusinessCommandState(existingRecord.doc, incomingDocument)
    ) {
      return true;
    }
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

function isForwardReplicatedBusinessCommandState(existingDocument, incomingDocument) {
  const rank = (document) => {
    const status = String(document?.terminal_status || document?.status || '').trim().toLowerCase();
    if (document?.execution_phase === 'terminal'
      || ['completed', 'failed', 'rejected', 'cancelled', 'canceled', 'blocked'].includes(status)) {
      return 2;
    }
    if (document?.replication_phase === 'native_observed'
      || ['accepted', 'queued', 'running', 'in_progress'].includes(status)) {
      return 1;
    }
    return 0;
  };
  return rank(incomingDocument) > rank(existingDocument);
}

function isStaleReplicatedBusinessCommandState(existingDocument, incomingDocument) {
  const existingStatus = String(existingDocument?.status || '').trim().toLowerCase();
  const incomingStatus = String(incomingDocument?.status || '').trim().toLowerCase();
  if (!existingStatus || !incomingStatus || existingStatus === incomingStatus) return false;
  if (incomingStatus === 'pending_sync' && existingStatus !== 'pending_sync') return true;
  const terminal = new Set([
    'completed', 'failed', 'rejected', 'cancelled', 'canceled', 'blocked',
  ]);
  return terminal.has(existingStatus) && !terminal.has(incomingStatus);
}

function documentLwt(doc = {}, fallback = Date.now()) {
  const values = [
    Number(doc._meta?.lwt || 0),
    Number(doc.updated_at_ms || 0),
    Number(doc.updatedAtMs || 0),
  ].filter((value) => Number.isFinite(value) && value > 0);
  return values.length ? Math.max(...values) : Number(fallback || Date.now());
}

function isQuotaExceededError(error) {
  if (!error) return false;
  if (error.name === 'QuotaExceededError') return true;
  if (typeof error.code === 'number' && error.code === 22) return true;
  const message = String(error.message || '').toLowerCase();
  return message.includes('quota') || message.includes('storage full');
}

function dispatchStorageChange(databaseName, collection, success, replicationOrigin) {
  globalThis.dispatchEvent?.(new CustomEvent('ctox-rxdb-storage-change', {
    detail: {
      databaseName,
      collection,
      ids: Object.keys(success || {}),
      replicationOriginRole: String(replicationOrigin?.role || ''),
      atMs: Date.now(),
    },
  }));
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

function shouldUsePushableReplicationIndex(excludedOriginRole) {
  return excludedOriginRole === 'ctox_instance';
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
  normalizeStoredReplicationFlags,
  normalizeSchemaIndexes,
  canUseBoundedCollectionCursor,
  encodeIndexValue,
  primaryKeyCandidateIds,
  replicationScanLimit,
  schemaIndexEntriesFor,
  schemaIndexQueryPlanFor,
  selectBestIndex,
  shouldUsePushableReplicationIndex,
  shouldAcceptDocumentWrite,
};
