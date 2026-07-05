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
    request.onblocked = () => resolve();
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
  }

  async addCollections(collections) {
    for (const [name, definition] of Object.entries(collections || {})) {
      if (this.collections[name]) continue;
      const schema = definition?.schema || definition;
      const collection = new CtoxRxCollection({
        name,
        schema,
        storageCollection: this.storage.collection(name, { schema }),
      });
      this.collections[name] = collection;
      this[name] = collection;
    }
    return this.collections;
  }

  collection(name) {
    return this.collections[name] || this[name] || null;
  }

  async close() {
    this.storage.close();
  }
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
            throw error;
          }
          if (!active) return;
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
        flushInitial(); // initial emission is immediate, not debounced
        const unsubscribe = this.observe(emit);
        return {
          unsubscribe: () => {
            active = false;
            if (pendingTimer != null) {
              clearTimeout(pendingTimer);
              pendingTimer = null;
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
  const source = pattern instanceof RegExp ? pattern : new RegExp(String(pattern));
  return source.test(String(actual));
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
  return String(path || '').split('.').reduce((value, key) => value?.[key], doc);
}

function setValueAtPath(doc, path, value) {
  const parts = String(path || '').split('.').filter(Boolean);
  if (!parts.length) return;
  let target = doc;
  for (const part of parts.slice(0, -1)) {
    if (!target[part] || typeof target[part] !== 'object') {
      target[part] = {};
    }
    target = target[part];
  }
  target[parts[parts.length - 1]] = value;
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
