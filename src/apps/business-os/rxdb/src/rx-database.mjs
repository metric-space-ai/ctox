import { openCtoxIndexedDbStorage } from './storage-indexeddb.mjs';
import {
  CTOX_CHECKPOINT_EPOCH_CAPABILITY,
  CTOX_BUSINESS_OS_SCHEMA_HASHES,
  CTOX_PEER_SESSION_CAPABILITY,
  CTOX_PROTOCOL_ERROR_CODES,
  CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
  CTOX_RXDB_PROTOCOL,
  CTOX_SCHEMA_HASH_SOURCES,
  CTOX_SCHEMA_HASH_CAPABILITY,
  buildProtocolPayload,
  canonicalJson,
  schemaHash,
  schemaHashSource,
  sha256Hex,
} from './schema.mjs';
import {
  getConnectionHandlerSimplePeer,
  replicateWebRTC,
} from './replication-webrtc.mjs';
import { getActiveCollectionRegistry } from './active-collections.mjs';
import { getPresenceRegistry } from './presence.mjs';
import { getMultiTabSyncCoordinator } from './multi-tab-sync-coordinator.mjs';

export function getCtoxIndexedDbStorage() {
  return { name: 'ctox-indexeddb-native' };
}

export async function createRxDatabase({
  name,
  storage = getCtoxIndexedDbStorage(),
  multiInstance = false,
  closeDuplicates = true,
} = {}) {
  if (!name) {
    throw new Error('createRxDatabase requires a name');
  }
  const nativeStorage = storage?.nativeStorage || await openCtoxIndexedDbStorage({ databaseName: name });
  return new CtoxRxDatabase({
    name,
    storage: nativeStorage,
    multiInstance,
    closeDuplicates,
  });
}

export async function removeRxDatabase(name) {
  if (!name || !globalThis.indexedDB?.deleteDatabase) return;
  await new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase(name);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error || new Error(`Failed to delete IndexedDB ${name}`));
    request.onblocked = () => reject(new Error(`IndexedDB delete blocked for ${name}`));
  });
}

export function addRxPlugin(_ignored = null) {
  return undefined;
}

export const RxDBMigrationSchemaPlugin = {
  name: 'ctox-JS-migration-schema-placeholder',
};

export function rxdbCore() {
  return {
    CTOX_CHECKPOINT_EPOCH_CAPABILITY,
    CTOX_BUSINESS_OS_SCHEMA_HASHES,
    CTOX_PEER_SESSION_CAPABILITY,
    CTOX_PROTOCOL_ERROR_CODES,
    CTOX_REQUIRED_PROTOCOL_CAPABILITIES,
    CTOX_RXDB_PROTOCOL,
    CTOX_SCHEMA_HASH_SOURCES,
    CTOX_SCHEMA_HASH_CAPABILITY,
    addRxPlugin,
    buildProtocolPayload,
    canonicalJson,
    createRxDatabase,
    getCtoxIndexedDbStorage,
    getConnectionHandlerSimplePeer,
    getMultiTabSyncCoordinator,
    getPresenceRegistry,
    replicateWebRTC,
    removeRxDatabase,
    RxDBMigrationSchemaPlugin,
    schemaHash,
    schemaHashSource,
    sha256Hex,
  };
}

class CtoxRxDatabase {
  constructor({ name, storage, multiInstance, closeDuplicates }) {
    this.name = name;
    this.storage = storage;
    this.multiInstance = Boolean(multiInstance);
    this.closeDuplicates = Boolean(closeDuplicates);
    this.collections = {};
    const journal = storage?.recoveryJournal || null;
    this.recovery = {
      getStatus: () => journal?.getStatus?.() || Promise.resolve(emptyRecoveryStatus(name)),
      export: (passphrase) => journal?.export?.(passphrase),
      previewImport: (file, passphrase) => journal?.previewImport?.(file, passphrase),
      applyImport: (previewId) => journal?.applyImport?.(previewId),
      retryPrimaryOpen: async () => {
        if (!globalThis.indexedDB?.open) return false;
        const request = indexedDB.open(name);
        const opened = await new Promise((resolve, reject) => {
          request.onsuccess = () => resolve(request.result);
          request.onerror = () => reject(request.error || new Error(`Failed to reopen IndexedDB ${name}`));
          request.onblocked = () => reject(new Error(`IndexedDB open blocked for ${name}`));
        });
        opened.close();
        return true;
      },
    };
    this.conflicts = {
      list: () => journal?.listConflicts?.() || Promise.resolve([]),
      resolve: (id, resolution) => journal?.resolveConflict?.(id, resolution),
    };
  }

  async addCollections(collections) {
    for (const [name, definition] of Object.entries(collections || {})) {
      if (this.collections[name]) continue;
      const schema = definition?.schema || definition;
      // `conflictStrategy` is a SIBLING of `schema` in the collection
      // definition ('lww' default, 'field-merge' opt-in) — outside the schema
      // object on purpose, so schema hashes are unaffected.
      const conflictStrategy = definition?.conflictStrategy;
      const collection = new CtoxRxCollection({
        name,
        schema,
        storageCollection: this.storage.collection(name, { schema, conflictStrategy }),
      });
      this.collections[name] = collection;
      this[name] = collection;
      // Registering schemas is on the shell's critical startup path. Recovery
      // must start immediately, but it must not hold every read-only surface
      // behind a potentially large journal replay. All mutating collection
      // methods call initializeRecovery() themselves and therefore still
      // fail closed until recovery has completed.
      collection.recoveryInitialization = Promise.resolve(
        collection.storageCollection.initializeRecovery?.(),
      ).catch((error) => {
        collection.recoveryInitializationError = error;
        console.error(`[ctox-rxdb] recovery initialization failed for ${name}:`, error);
        return null;
      });
    }
    return this.collections;
  }

  collection(name) {
    return this.collections[name] || this[name] || null;
  }

  async getUnsyncedWriteSummary() {
    return this.storage.unsyncedWriteSummary?.() || { total: 0, byCollection: {} };
  }

  async close() {
    for (const collection of Object.values(this.collections)) {
      collection.storageCollection?.close?.();
    }
    this.storage.close();
  }
}

function emptyRecoveryStatus(databaseName) {
  return {
    schema: 'ctox.browser-recovery.status.v2',
    databaseName,
    pendingBatches: 0,
    pendingWrites: 0,
    pendingBytes: 0,
    oldestPendingAtMs: 0,
    unresolvedConflicts: 0,
    lastExportAtMs: 0,
    updatedAtMs: Date.now(),
  };
}

class CtoxRxCollection {
  constructor({ name, schema, storageCollection }) {
    this.name = name;
    this.schema = {
      jsonSchema: schema,
      version: schema?.version || 0,
      primaryPath: primaryPathFromSchema(schema),
      hash: () => schemaHash(schema, name),
    };
    this.storageCollection = storageCollection;
    this.demandLoader = null;
    this.liveQueryPerformanceStats = {
      complexLiveQueryReexecs: 0,
      deltaLiveQueryApplies: 0,
      lastComplexLiveQuery: null,
      lastDeltaLiveQuery: null,
    };
  }

  setDemandLoader(loader) {
    this.demandLoader = loader || null;
  }

  async insert(doc) {
    const normalized = normalizeDoc(doc, this.schema.primaryPath);
    await this.storageCollection.bulkWrite([normalized]);
    return new CtoxRxDocument(this, normalized);
  }

  async bulkInsert(docs = []) {
    if (!Array.isArray(docs)) {
      throw new TypeError('bulkInsert expects an array of documents');
    }
    const normalized = docs.map((doc) => normalizeDoc(doc, this.schema.primaryPath));
    await this.storageCollection.bulkWrite(normalized);
    return normalized.map((doc) => new CtoxRxDocument(this, doc));
  }

  async upsert(doc) {
    const normalized = normalizeDoc(doc, this.schema.primaryPath);
    const written = await this.storageCollection.upsert(normalized);
    return new CtoxRxDocument(this, written);
  }

  async atomicUpsert(doc) {
    return this.upsert(doc);
  }

  async bulkUpsert(docs = []) {
    if (!Array.isArray(docs)) {
      throw new TypeError('bulkUpsert expects an array of documents');
    }
    const normalized = docs.map((doc) => normalizeDoc(doc, this.schema.primaryPath));
    const result = typeof this.storageCollection.bulkUpsert === 'function'
      ? await this.storageCollection.bulkUpsert(normalized)
      : await this.storageCollection.bulkWrite(normalized);
    const success = result?.success || {};
    return normalized.map((doc) => new CtoxRxDocument(this, success[doc.id] || doc));
  }

  find(query = {}) {
    return new CtoxRxQuery(this, query, false);
  }

  findOne(idOrQuery) {
    return new CtoxRxQuery(this, idOrQuery, true);
  }

  count(query = {}) {
    return {
      exec: async () => {
        const normalized = normalizeQuery(query, this.schema.primaryPath);
        if (typeof this.storageCollection.countDocuments === 'function') {
          return this.storageCollection.countDocuments(normalized, {
            matchesSelector,
            sortDocuments,
          });
        }
        return (await this.find(query).exec()).length;
      },
    };
  }

  schemaIndexes() {
    return this.storageCollection.schemaIndexes?.() || [];
  }

  queryPlanFor(query = {}) {
    const normalized = normalizeQuery(query, this.schema.primaryPath);
    return this.storageCollection.queryPlanFor?.(normalized) || {
      collection: this.name,
      indexed: false,
      selectorFields: Object.keys(normalized.selector || {}),
      sortFields: normalizeSort(normalized.sort).map((entry) => Object.keys(entry)[0]).filter(Boolean),
      selectedIndex: null,
    };
  }

  setQueryPerformancePolicy(policy = {}) {
    this.storageCollection.setQueryPerformancePolicy?.(policy);
  }

  resetQueryPerformanceStats() {
    this.storageCollection.resetQueryPerformanceStats?.();
    this.liveQueryPerformanceStats = {
      complexLiveQueryReexecs: 0,
      deltaLiveQueryApplies: 0,
      lastComplexLiveQuery: null,
      lastDeltaLiveQuery: null,
    };
  }

  getQueryPerformanceStats() {
    return {
      storage: this.storageCollection.getQueryPerformanceStats?.() || null,
      liveQueries: cloneJson(this.liveQueryPerformanceStats),
    };
  }

  recordComplexLiveQueryReexec(query = {}) {
    this.liveQueryPerformanceStats.complexLiveQueryReexecs += 1;
    this.liveQueryPerformanceStats.lastComplexLiveQuery = {
      at: Date.now(),
      selectorFields: Object.keys(query?.selector || {}).filter((field) => !field.startsWith('$')),
      sortFields: normalizeSort(query?.sort || []).map((entry) => Object.keys(entry || {})[0]).filter(Boolean),
      limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : null,
      skip: Number.isFinite(Number(query?.skip)) ? Number(query.skip) : 0,
    };
  }

  recordDeltaLiveQueryApply(query = {}, changedCount = 0) {
    this.liveQueryPerformanceStats.deltaLiveQueryApplies += 1;
    this.liveQueryPerformanceStats.lastDeltaLiveQuery = {
      at: Date.now(),
      changedCount,
      selectorFields: Object.keys(query?.selector || {}).filter((field) => !field.startsWith('$')),
      sortFields: normalizeSort(query?.sort || []).map((entry) => Object.keys(entry || {})[0]).filter(Boolean),
      limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : null,
      skip: Number.isFinite(Number(query?.skip)) ? Number(query.skip) : 0,
    };
  }

  observe(listener) {
    return this.storageCollection.observe(listener);
  }

  get $() {
    return {
      subscribe: (listener) => {
        let active = true;
        // Phase 2: a live collection subscription marks this collection as
        // foreground in the RxDB layer so replication prioritizes it on the
        // shared DataChannel. Released on unsubscribe.
        const registry = getActiveCollectionRegistry();
        registry.subscriptionStarted(this.name);
        // V1.5 production hardening: debounce change-bulks so a busy
        // collection (1000 writes/sec) doesn't trigger 1000 subscription
        // emissions per second. The 50 ms window collapses burst writes
        // into one re-evaluation. See docs/rxdb_on-demand-load.md Wave 3.
        let pendingTimer = null;
        let initialRetryTimer = null;
        let initialRetryAttempt = 0;
        let initialized = false;
        let pendingSuccess = {};
        const documentsById = new Map();
        const debounceMs = OBSERVABLE_DEBOUNCE_MS;
        const emitSnapshot = () => {
          listener({
            collectionName: this.name,
            documents: Array.from(documentsById.values()),
          });
        };
        const applySuccess = (success = {}) => {
          for (const rawDoc of Object.values(success || {})) {
            const id = documentIdFromDoc(rawDoc);
            if (!id) continue;
            if (rawDoc?._deleted) {
              documentsById.delete(id);
            } else {
              documentsById.set(id, new CtoxRxDocument(this, rawDoc));
            }
          }
        };
        const flushInitial = async () => {
          if (!active) return;
          let documents;
          try {
            documents = await this.find().exec();
          } catch (error) {
            if (isIndexedDbConnectionClosingError(error)) return;
            if (active && isRetryableObservableInitError(error)) {
              const delayMs = observableInitRetryDelayMs(initialRetryAttempt);
              initialRetryAttempt += 1;
              initialRetryTimer = setTimeout(() => {
                initialRetryTimer = null;
                void flushInitial();
              }, delayMs);
              return;
            }
            throw error;
          }
          if (!active) return;
          initialRetryAttempt = 0;
          documentsById.clear();
          for (const doc of documents) {
            const id = documentIdFromDoc(doc);
            if (id) documentsById.set(id, doc);
          }
          applySuccess(pendingSuccess);
          pendingSuccess = {};
          initialized = true;
          emitSnapshot();
        };
        const flushDelta = () => {
          pendingTimer = null;
          if (!active || !initialized) return;
          applySuccess(pendingSuccess);
          pendingSuccess = {};
          emitSnapshot();
        };
        const emit = (event) => {
          pendingSuccess = {
            ...pendingSuccess,
            ...successPayloadFromChangeEvent(event),
          };
          if (!initialized) return;
          if (pendingTimer != null) return;
          pendingTimer = setTimeout(flushDelta, debounceMs);
        };
        // Initial reads can briefly hit the bounded demand-transport queue when
        // a shell activates many collections at once. Treat that retryable
        // backpressure as flow control, not as an unhandled page error.
        void flushInitial(); // initial emission is immediate, not debounced
        const unsubscribe = this.observe(emit);
        return {
          unsubscribe: () => {
            active = false;
            if (pendingTimer != null) {
              clearTimeout(pendingTimer);
              pendingTimer = null;
            }
            if (initialRetryTimer != null) {
              clearTimeout(initialRetryTimer);
              initialRetryTimer = null;
            }
            unsubscribe();
            registry.subscriptionEnded(this.name);
          },
        };
      },
    };
  }
}

export const OBSERVABLE_DEBOUNCE_MS = 50;
export const OBSERVABLE_INIT_RETRY_BASE_MS = 100;
export const OBSERVABLE_INIT_RETRY_MAX_MS = 2_000;

function isRetryableObservableInitError(error) {
  if (error?.retryable === true) return true;
  const code = String(error?.code || '').trim().toUpperCase();
  if (code === 'QUERY_QUEUE_LIMIT') return true;
  return String(error?.message || error || '').includes('QUERY_QUEUE_LIMIT:');
}

function observableInitRetryDelayMs(attempt = 0) {
  const exponent = Math.max(0, Math.min(8, Number(attempt) || 0));
  return Math.min(
    OBSERVABLE_INIT_RETRY_MAX_MS,
    OBSERVABLE_INIT_RETRY_BASE_MS * (2 ** exponent),
  );
}

function isIndexedDbConnectionClosingError(error) {
  const message = String(error?.message || error || '');
  return error?.name === 'InvalidStateError'
    && message.includes('database connection is closing');
}

class CtoxRxQuery {
  constructor(collection, query, single) {
    this.collection = collection;
    this.query = normalizeQuery(query, collection.schema.primaryPath);
    this.single = single;
    this.$ = {
      subscribe: (listener) => {
        let active = true;
        // Phase 2: a live query subscription marks its collection foreground.
        const registry = getActiveCollectionRegistry();
        registry.subscriptionStarted(this.collection.name);
        let pendingTimer = null;
        let initialized = false;
        let pendingPrimaryDoc = undefined;
        const primaryId = this.single
          ? singlePrimaryKeyCandidateId(this.query, this.collection.schema.primaryPath)
          : '';
        const canApplyPrimaryDelta = Boolean(primaryId);
        const canApplyQueryDelta = !this.single && canApplyUnboundedQueryDelta(this.query);
        let pendingSuccess = {};
        const queryDocumentsById = new Map();
        const emitQueryDocuments = () => {
          listener(sortDocuments(Array.from(queryDocumentsById.values()), this.query.sort));
        };
        const applyQuerySuccess = (success = {}) => {
          for (const rawDoc of Object.values(success || {})) {
            const id = documentIdFromDoc(rawDoc);
            if (!id) continue;
            if (rawDoc?._deleted || !matchesSelector(rawDoc, this.query.selector)) {
              queryDocumentsById.delete(id);
            } else {
              queryDocumentsById.set(id, new CtoxRxDocument(this.collection, rawDoc));
            }
          }
        };
        const flushEmit = () => {
          pendingTimer = null;
          if (!active) return;
          if (initialized && !canApplyPrimaryDelta && !canApplyQueryDelta) {
            this.collection.recordComplexLiveQueryReexec(this.query);
          }
          this.exec()
            .then((value) => {
              if (!active) return;
              initialized = true;
              if (pendingPrimaryDoc !== undefined && canApplyPrimaryDelta) {
                listener(wrapPrimaryDeltaDocument(this.collection, pendingPrimaryDoc));
                pendingPrimaryDoc = undefined;
                return;
              }
              if (canApplyQueryDelta && Array.isArray(value)) {
                queryDocumentsById.clear();
                for (const doc of value) {
                  const id = documentIdFromDoc(doc);
                  if (id) queryDocumentsById.set(id, doc);
                }
                if (Object.keys(pendingSuccess).length > 0) {
                  applyQuerySuccess(pendingSuccess);
                  pendingSuccess = {};
                  emitQueryDocuments();
                  return;
                }
              }
              listener(value);
            })
            .catch(() => {});
        };
        const flushPrimaryDelta = () => {
          pendingTimer = null;
          if (!active || !initialized || !canApplyPrimaryDelta || pendingPrimaryDoc === undefined) return;
          const next = pendingPrimaryDoc;
          pendingPrimaryDoc = undefined;
          listener(wrapPrimaryDeltaDocument(this.collection, next));
        };
        const flushQueryDelta = () => {
          pendingTimer = null;
          if (!active || !initialized || !canApplyQueryDelta) return;
          const success = pendingSuccess;
          pendingSuccess = {};
          applyQuerySuccess(success);
          this.collection.recordDeltaLiveQueryApply(this.query, Object.keys(success).length);
          emitQueryDocuments();
        };
        const emit = (event) => {
          if (canApplyPrimaryDelta) {
            const success = successPayloadFromChangeEvent(event);
            if (!Object.prototype.hasOwnProperty.call(success, primaryId)) return;
            pendingPrimaryDoc = success[primaryId] || null;
            if (!initialized) return;
            if (pendingTimer != null) return;
            pendingTimer = setTimeout(flushPrimaryDelta, 50);
            return;
          }
          if (canApplyQueryDelta) {
            pendingSuccess = {
              ...pendingSuccess,
              ...successPayloadFromChangeEvent(event),
            };
            if (!initialized) return;
            if (pendingTimer != null) return;
            pendingTimer = setTimeout(flushQueryDelta, 50);
            return;
          }
          if (pendingTimer != null) return;
          pendingTimer = setTimeout(flushEmit, 50);
        };
        flushEmit();
        const unsubscribe = this.collection.observe(emit);
        return {
          unsubscribe: () => {
            active = false;
            if (pendingTimer != null) {
              clearTimeout(pendingTimer);
              pendingTimer = null;
            }
            unsubscribe();
            registry.subscriptionEnded(this.collection.name);
          },
        };
      },
    };
  }

  selector(selector = {}) {
    return this._clone({ selector });
  }

  sort(sort = []) {
    return this._clone({ sort: normalizeSort(sort) });
  }

  limit(limit) {
    return this._clone({ limit: normalizePositiveInteger(limit, 'limit') });
  }

  skip(skip) {
    return this._clone({ skip: normalizePositiveInteger(skip, 'skip') });
  }

  where(field) {
    if (!field || typeof field !== 'string') {
      throw new TypeError('where(field) requires a non-empty field path');
    }
    const withOperator = (operator, value) => {
      const current = this.query.selector?.[field];
      const nextValue = current && typeof current === 'object' && !Array.isArray(current)
        ? { ...current, [operator]: value }
        : { [operator]: value };
      return this._withSelectorPatch({ [field]: nextValue });
    };
    return {
      eq: (value) => this._withSelectorPatch({ [field]: value }),
      ne: (value) => withOperator('$ne', value),
      gt: (value) => withOperator('$gt', value),
      gte: (value) => withOperator('$gte', value),
      lt: (value) => withOperator('$lt', value),
      lte: (value) => withOperator('$lte', value),
      in: (value) => withOperator('$in', value),
      nin: (value) => withOperator('$nin', value),
      exists: (value = true) => withOperator('$exists', value),
      regex: (value) => withOperator('$regex', value),
    };
  }

  async exec() {
    // Phase 2: an imperative `.exec()` read keeps the collection foreground for
    // a short window so one-shot reads also get priority on the wire.
    getActiveCollectionRegistry().markRead(this.collection.name);
    let docs;
    if (this.collection.demandLoader) {
      const demandOptions = this.single && !Number.isFinite(Number(this.query.limit))
        ? { window: { offset: Number(this.query.skip || 0), limit: 1 } }
        : undefined;
      docs = await this.collection.demandLoader.resolveQuery(this.query, demandOptions);
    } else if (typeof this.collection.storageCollection.queryDocuments === 'function') {
      docs = await this.collection.storageCollection.queryDocuments(this.query, {
        matchesSelector,
        sortDocuments,
      });
    } else {
      docs = await this.collection.storageCollection.allDocuments();
      docs = docs.filter((doc) => matchesSelector(doc, this.query.selector));
      docs = sortDocuments(docs, this.query.sort);
      if (Number.isFinite(this.query.skip) && this.query.skip > 0) {
        docs = docs.slice(this.query.skip);
      }
      if (Number.isFinite(this.query.limit)) {
        docs = docs.slice(0, this.query.limit);
      }
    }
    const wrapped = docs.map((doc) => new CtoxRxDocument(this.collection, doc));
    return this.single ? wrapped[0] || null : wrapped;
  }

  _clone(patch = {}) {
    return new CtoxRxQuery(this.collection, {
      selector: patch.selector ?? this.query.selector,
      sort: patch.sort ?? this.query.sort,
      limit: patch.limit ?? this.query.limit,
      skip: patch.skip ?? this.query.skip,
    }, this.single);
  }

  _withSelectorPatch(patch = {}) {
    return this._clone({
      selector: {
        ...(this.query.selector || {}),
        ...patch,
      },
    });
  }
}

class CtoxRxDocument {
  constructor(collection, data) {
    this.collection = collection;
    this._data = { ...data };
    Object.assign(this, this._data);
  }

  toJSON() {
    return { ...this._data };
  }

  async patch(fields) {
    return this.incrementalPatch(fields);
  }

  async atomicPatch(fields) {
    return this.incrementalPatch(fields);
  }

  async update(operation) {
    if (operation?.$set && typeof operation.$set === 'object') {
      return this.incrementalPatch(operation.$set);
    }
    return this.incrementalPatch(operation || {});
  }

  async incrementalModify(modifier) {
    const current = this.toJSON();
    const next = await modifier({ ...current });
    return this.incrementalPatch(next || current);
  }

  async atomicUpdate(modifier) {
    return this.incrementalModify(modifier);
  }

  async incrementalPatch(fields) {
    const updatedAtMs = Number(fields?.updated_at_ms || Date.now());
    const next = {
      ...this._data,
      ...fields,
      updated_at_ms: updatedAtMs,
      _meta: {
        ...(this._data._meta || {}),
        ...(fields?._meta || {}),
        lwt: updatedAtMs,
      },
    };
    await this.collection.storageCollection.upsert(next);
    this._data = next;
    Object.assign(this, next);
    return this;
  }

  async remove() {
    await this.incrementalPatch({ _deleted: true, is_deleted: true, updated_at_ms: Date.now() });
    return this;
  }
}

function normalizeQuery(query, primaryPath) {
  if (typeof query === 'string') {
    return { selector: { [primaryPath]: query } };
  }
  if (query && typeof query === 'object' && !query.selector && Object.keys(query).length && !query.sort && !query.limit && !query.skip) {
    return { selector: query };
  }
  return {
    selector: query?.selector || {},
    sort: normalizeSort(query?.sort),
    limit: Number.isFinite(Number(query?.limit)) ? Number(query.limit) : undefined,
    skip: Number.isFinite(Number(query?.skip)) ? Math.max(0, Number(query.skip)) : undefined,
  };
}

function matchesSelector(doc, selector = {}) {
  for (const [key, expected] of Object.entries(selector || {})) {
    if (key === '$and') {
      if (!Array.isArray(expected) || !expected.every((item) => matchesSelector(doc, item))) return false;
      continue;
    }
    if (key === '$or') {
      if (!Array.isArray(expected) || !expected.some((item) => matchesSelector(doc, item))) return false;
      continue;
    }
    if (key === '$not') {
      if (matchesSelector(doc, expected)) return false;
      continue;
    }
    const actual = valueAtPath(doc, key);
    if (expected && typeof expected === 'object' && !Array.isArray(expected)) {
      if ('$in' in expected && !isInOperatorMatch(actual, expected.$in)) return false;
      if ('$nin' in expected && isInOperatorMatch(actual, expected.$nin)) return false;
      if ('$eq' in expected && actual !== expected.$eq) return false;
      if ('$ne' in expected && actual === expected.$ne) return false;
      if ('$gt' in expected && !(actual > expected.$gt)) return false;
      if ('$gte' in expected && !(actual >= expected.$gte)) return false;
      if ('$lt' in expected && !(actual < expected.$lt)) return false;
      if ('$lte' in expected && !(actual <= expected.$lte)) return false;
      if ('$exists' in expected && (actual !== undefined) !== Boolean(expected.$exists)) return false;
      if ('$regex' in expected && !matchesRegex(actual, expected.$regex)) return false;
      if ('$contains' in expected && !arrayContains(actual, expected.$contains)) return false;
      if ('$elemMatch' in expected && !elemMatch(actual, expected.$elemMatch)) return false;
      continue;
    }
    if (actual !== expected) return false;
  }
  return true;
}

function sortDocuments(docs, sort = []) {
  if (!sort.length) return docs;
  return docs.slice().sort((left, right) => {
    for (const entry of sort) {
      const [key, direction] = Object.entries(entry)[0] || [];
      const factor = direction === 'desc' ? -1 : 1;
      const a = valueAtPath(left, key);
      const b = valueAtPath(right, key);
      if (a < b) return -1 * factor;
      if (a > b) return 1 * factor;
    }
    return 0;
  });
}

function normalizeSort(sort = []) {
  if (!sort) return [];
  if (typeof sort === 'string') return [{ [sort]: 'asc' }];
  if (!Array.isArray(sort)) return [];
  return sort.map((entry) => {
    if (typeof entry === 'string') return { [entry]: 'asc' };
    if (!entry || typeof entry !== 'object') return {};
    const [key, direction] = Object.entries(entry)[0] || [];
    if (!key) return {};
    return { [key]: normalizeSortDirection(direction) };
  }).filter((entry) => Object.keys(entry).length);
}

function normalizeSortDirection(direction) {
  if (direction === -1 || direction === 'desc' || direction === 'DESC') return 'desc';
  return 'asc';
}

function normalizePositiveInteger(value, name) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new TypeError(`${name} must be a positive number`);
  }
  return Math.floor(parsed);
}

function successPayloadFromChangeEvent(event) {
  return event?.success && typeof event.success === 'object'
    ? event.success
    : event?.detail?.success && typeof event.detail.success === 'object'
      ? event.detail.success
      : {};
}

function documentIdFromDoc(doc) {
  return String(doc?.id || doc?._id || doc?.document_id || doc?.documentId || '').trim();
}

function cloneJson(value) {
  return value == null ? value : JSON.parse(JSON.stringify(value));
}

function singlePrimaryKeyCandidateId(query = {}, primaryPath = 'id') {
  const selector = query?.selector || {};
  for (const field of ['id', '_id', primaryPath].filter(Boolean)) {
    if (!Object.prototype.hasOwnProperty.call(selector, field)) continue;
    const value = selector[field];
    if (typeof value === 'string' || typeof value === 'number') return String(value);
    if (value && typeof value === 'object' && !Array.isArray(value) && '$eq' in value && value.$eq != null) {
      return String(value.$eq);
    }
    return '';
  }
  return '';
}

function canApplyUnboundedQueryDelta(query = {}) {
  return !Number.isFinite(Number(query?.limit))
    && !(Number.isFinite(Number(query?.skip)) && Number(query.skip) > 0);
}

function wrapPrimaryDeltaDocument(collection, doc) {
  if (!doc || doc._deleted) return null;
  return new CtoxRxDocument(collection, doc);
}

function isInOperatorMatch(actual, candidates) {
  const values = Array.isArray(candidates) ? candidates : [candidates];
  if (Array.isArray(actual)) {
    return actual.some((value) => values.includes(value));
  }
  return values.includes(actual);
}

function matchesRegex(actual, pattern) {
  if (actual === undefined || actual === null) return false;
  const compiled = compileLinearRegexPattern(pattern);
  if (!compiled) return false;
  return testLinearRegexPattern(String(actual), compiled);
}

const MAX_LINEAR_REGEX_PATTERN_LENGTH = 128;
const MAX_LINEAR_REGEX_INPUT_LENGTH = 8192;

function compileLinearRegexPattern(pattern) {
  const source = pattern instanceof RegExp ? pattern.source : String(pattern ?? '');
  const ignoreCase = pattern instanceof RegExp ? pattern.ignoreCase : false;
  if (!source || source.length > MAX_LINEAR_REGEX_PATTERN_LENGTH) return null;
  let cursor = 0;
  let end = source.length;
  const anchoredStart = source[cursor] === '^';
  if (anchoredStart) cursor += 1;
  const anchoredEnd = end > cursor && source[end - 1] === '$' && !isEscaped(source, end - 1);
  if (anchoredEnd) end -= 1;

  const tokens = [];
  while (cursor < end) {
    const parsed = parseLinearRegexAtom(source, cursor, end);
    if (!parsed) return null;
    cursor = parsed.next;
    let min = 1;
    let max = 1;
    if (cursor < end && ['*', '+', '?'].includes(source[cursor])) {
      const quantifier = source[cursor];
      min = quantifier === '+' ? 1 : 0;
      max = quantifier === '?' ? 1 : Infinity;
      cursor += 1;
    }
    tokens.push({ ...parsed.atom, min, max });
  }
  return { tokens, anchoredStart, anchoredEnd, ignoreCase };
}

function parseLinearRegexAtom(source, cursor, end) {
  const char = source[cursor];
  if (!char) return null;
  if (char === '.') {
    return { atom: { kind: 'any' }, next: cursor + 1 };
  }
  if (char === '\\') {
    const escaped = source[cursor + 1];
    if (!escaped || cursor + 1 >= end) return null;
    if (escaped === 's') return { atom: { kind: 'space' }, next: cursor + 2 };
    if (escaped === 'd') return { atom: { kind: 'digit' }, next: cursor + 2 };
    if (escaped === 'w') return { atom: { kind: 'word' }, next: cursor + 2 };
    return { atom: { kind: 'literal', value: escaped }, next: cursor + 2 };
  }
  if ('()[]{}|'.includes(char)) return null;
  if ('*+?'.includes(char)) return null;
  return { atom: { kind: 'literal', value: char }, next: cursor + 1 };
}

function testLinearRegexPattern(value, compiled) {
  const input = String(value || '').slice(0, MAX_LINEAR_REGEX_INPUT_LENGTH);
  const text = compiled.ignoreCase ? input.toLocaleLowerCase() : input;
  const tokens = compiled.ignoreCase
    ? compiled.tokens.map((token) => token.kind === 'literal' ? { ...token, value: token.value.toLocaleLowerCase() } : token)
    : compiled.tokens;
  if (!tokens.length) return true;
  const starts = compiled.anchoredStart ? [0] : Array.from({ length: text.length + 1 }, (_, index) => index);
  return starts.some((start) => {
    const endings = consumeLinearRegexTokens(text, tokens, start, 0);
    return endings.some((end) => compiled.anchoredEnd ? end === text.length : true);
  });
}

function consumeLinearRegexTokens(text, tokens, position, tokenIndex) {
  if (tokenIndex >= tokens.length) return [position];
  const token = tokens[tokenIndex];
  const endings = [];
  let next = position;
  let count = 0;
  while (count < token.min) {
    if (!linearRegexAtomMatches(text[next], token)) return endings;
    next += 1;
    count += 1;
  }
  const positions = [next];
  while (count < token.max && next < text.length && linearRegexAtomMatches(text[next], token)) {
    next += 1;
    count += 1;
    positions.push(next);
  }
  for (let index = positions.length - 1; index >= 0; index -= 1) {
    endings.push(...consumeLinearRegexTokens(text, tokens, positions[index], tokenIndex + 1));
  }
  return endings;
}

function linearRegexAtomMatches(char, token) {
  if (char === undefined) return false;
  if (token.kind === 'any') return true;
  if (token.kind === 'space') return /\s/.test(char);
  if (token.kind === 'digit') return char >= '0' && char <= '9';
  if (token.kind === 'word') return /[A-Za-z0-9_]/.test(char);
  return char === token.value;
}

function isEscaped(source, index) {
  let slashCount = 0;
  for (let cursor = index - 1; cursor >= 0 && source[cursor] === '\\'; cursor -= 1) {
    slashCount += 1;
  }
  return slashCount % 2 === 1;
}

function arrayContains(actual, expected) {
  return Array.isArray(actual) && actual.includes(expected);
}

function elemMatch(actual, selector) {
  return Array.isArray(actual) && actual.some((item) => (
    item && typeof item === 'object'
      ? matchesSelector(item, selector)
      : item === selector
  ));
}

function valueAtPath(doc, path) {
  const parts = pathSegments(path);
  if (parts.some(isUnsafePathSegment)) return undefined;
  return parts.reduce((value, key) => value?.[key], doc);
}

function setValueAtPath(doc, path, value) {
  const parts = assertSafePathSegments(path, 'document path');
  if (!parts.length) return;
  let target = doc;
  for (const part of parts.slice(0, -1)) {
    let next = ownValue(target, part);
    if (!next || typeof next !== 'object') {
      next = {};
      defineOwnValue(target, part, next);
    }
    target = next;
  }
  defineOwnValue(target, parts[parts.length - 1], value);
}

function pathSegments(path) {
  return String(path || '').split('.').filter(Boolean);
}

function isUnsafePathSegment(segment) {
  return segment === '__proto__' || segment === 'prototype' || segment === 'constructor';
}

function assertSafePathSegments(path, label) {
  const parts = pathSegments(path);
  if (parts.some(isUnsafePathSegment)) {
    throw new Error(`${label} contains unsafe prototype segment`);
  }
  return parts;
}

function ownValue(object, key) {
  if (!object || typeof object !== 'object' || !Object.hasOwn(object, key)) return undefined;
  return Object.getOwnPropertyDescriptor(object, key)?.value;
}

function defineOwnValue(object, key, value) {
  Object.defineProperty(object, key, {
    value,
    enumerable: true,
    configurable: true,
    writable: true,
  });
}

function primaryPathFromSchema(schema) {
  const primary = schema?.primaryKey;
  if (typeof primary === 'string') return primary;
  if (primary?.key) return primary.key;
  return 'id';
}

function normalizeDoc(doc, primaryPath) {
  if (!doc || typeof doc !== 'object') {
    throw new TypeError('document must be an object');
  }
  assertSafePathSegments(primaryPath, 'primary key path');
  const normalized = { ...doc };
  const id = normalized.id || normalized._id || valueAtPath(normalized, primaryPath);
  if (!id) {
    throw new Error(`document is missing primary key ${primaryPath}`);
  }
  normalized.id = String(id);
  if (valueAtPath(normalized, primaryPath) === undefined) {
    setValueAtPath(normalized, primaryPath, normalized.id);
  }
  normalized._deleted = Boolean(normalized._deleted);
  normalized._meta = {
    ...(normalized._meta || {}),
    lwt: documentLwt(normalized),
  };
  return normalized;
}

function documentLwt(doc = {}, fallback = Date.now()) {
  const values = [
    Number(doc._meta?.lwt || 0),
    Number(doc.updated_at_ms || 0),
    Number(doc.updatedAtMs || 0),
  ].filter((value) => Number.isFinite(value) && value > 0);
  return values.length ? Math.max(...values) : Number(fallback || Date.now());
}

export const ctoxRxdbTestInternals = {
  matchesSelector,
  normalizeDoc,
  normalizeQuery,
  normalizeSort,
  sortDocuments,
};
