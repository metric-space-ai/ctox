import { collections as matchingSchemas } from '../schema.js';

const DB_NAME = 'ctox_business_os_v3';

const COLLECTION_MAP = {
  sources: 'matching_requirements',
  requirements: 'matching_requirements',
  requirementSources: 'matching_requirements',
  objects: 'matching_objects',
  matches: 'matching_results',
  object_photo_chunks: 'matching_objects',
  object_photo_variant_chunks: 'matching_objects'
};

let dbPromise = null;
let rawDbPromise = null;
let injectedRawDatabase = null;
const cacheByCollection = new Map();
const hydratedRemoteCollections = new Set();

export function setBusinessOsRawDatabase(raw) {
  injectedRawDatabase = raw && typeof raw === 'object' ? raw : null;
  dbPromise = null;
  rawDbPromise = null;
  cacheByCollection.clear();
  hydratedRemoteCollections.clear();
}

export async function getContactsCollection() {
  return { database: await getDatabase() };
}

export async function getDatabase() {
  if (!dbPromise) dbPromise = createDatabase();
  return dbPromise;
}

async function createDatabase() {
  const raw = await getRawDatabase();
  const db = {};
  for (const name of Object.keys(COLLECTION_MAP)) {
    db[name] = createCollection(name, raw[COLLECTION_MAP[name]]);
  }
  return db;
}

async function getRawDatabase() {
  if (!rawDbPromise) rawDbPromise = createRawDatabase();
  return rawDbPromise;
}

async function createRawDatabase() {
  if (injectedRawDatabase) return injectedRawDatabase;
  const rxdb = await loadRxdb();
  const { createRxDatabase, getRxStorageDexie } = rxdb;
  const raw = await createRxDatabase({
    name: DB_NAME,
    storage: getRxStorageDexie(),
    multiInstance: true,
    closeDuplicates: true,
  });
  const missing = {};
  for (const [collectionName, definition] of Object.entries(matchingSchemas || {})) {
    if (!raw[collectionName]) missing[collectionName] = normalizeCollectionDefinition(definition);
  }
  if (Object.keys(missing).length) await raw.addCollections(missing);
  return raw;
}

function normalizeCollectionDefinition(definition) {
  if (definition?.schema) return definition;
  return { schema: definition };
}

function createCollection(name, rxCollection) {
  return {
    $: {
      subscribe(handler) {
        if (!rxCollection?.$?.subscribe) return { unsubscribe() {} };
        return rxCollection.$.subscribe((event) => {
          invalidateCacheForRemote(COLLECTION_MAP[name] || name);
          handler?.(event);
        });
      }
    },
    find(query = {}) {
      return {
        exec: async () => {
          const docs = await loadCollection(name);
          return queryDocuments(name, docs, query).map((doc) => wrapDoc(name, doc));
        }
      };
    },
    findOne(arg = {}) {
      return {
        exec: async () => {
          const docs = await loadCollection(name);
          const selector = typeof arg === 'string' ? { id: arg } : (arg.selector || arg || {});
          const found = queryDocuments(name, docs, { selector })[0] || null;
          return found ? wrapDoc(name, found) : null;
        }
      };
    },
    insert: async (doc) => wrapDoc(name, await saveDocument(name, doc)),
    upsert: async (doc) => wrapDoc(name, await saveDocument(name, doc)),
    atomicUpsert: async (doc) => wrapDoc(name, await saveDocument(name, doc))
  };
}

async function loadCollection(name, { force = false } = {}) {
  if (!force && cacheByCollection.has(name)) return cacheByCollection.get(name);
  const remote = COLLECTION_MAP[name] || name;
  const raw = await getRawDatabase();
  const rxCollection = raw[remote];
  if (rxCollection) await hydrateRemoteCollection(remote, rxCollection);
  const rows = rxCollection ? await rxCollection.find().exec() : [];
  const documents = rows.map((row) => fromPersistedDocument(name, row.toJSON()));
  const filtered = documents.filter((doc) => !doc?._deleted && belongsToUiCollection(name, doc));
  cacheByCollection.set(name, filtered);
  return filtered;
}

async function hydrateRemoteCollection(remote, rxCollection) {
  if (!remote || !rxCollection || hydratedRemoteCollections.has(remote)) return;
  hydratedRemoteCollections.add(remote);
  try {
    let checkpoint = null;
    for (;;) {
      const response = await fetch(`/api/business-os/rxdb/${encodeURIComponent(remote)}/pull`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ checkpoint, batch_size: 200 })
      });
      if (!response.ok) throw new Error(`Pull failed for ${remote}: ${response.status}`);
      const page = await response.json();
      const documents = Array.isArray(page?.documents)
        ? page.documents.map((doc) => normalizeRemoteDocument(remote, doc))
        : [];
      if (documents.length) {
        if (typeof rxCollection.bulkUpsert === 'function') {
          const result = await rxCollection.bulkUpsert(documents);
          const errors = Array.isArray(result?.error) ? result.error : [];
          if (errors.length) {
            for (const doc of documents) await rxCollection.upsert(doc);
          }
        } else {
          for (const doc of documents) await rxCollection.upsert(doc);
        }
      }
      checkpoint = page?.checkpoint || checkpoint;
      if (!page?.has_more || !documents.length) break;
    }
  } catch (error) {
    hydratedRemoteCollections.delete(remote);
    console.warn(`[matching] Initial CTOX pull failed for ${remote}`, error);
  }
}

function normalizeRemoteDocument(remote, input) {
  const doc = toPlain(input);
  const now = Date.now();
  const id = String(doc.id || doc.record_id || makeId(remote));
  const kind = safeText(doc.kind) || inferRemoteKind(remote);
  const data = doc.data && typeof doc.data === 'object'
    ? { ...doc.data, __ctox_kind: doc.data.__ctox_kind || kind }
    : { ...doc, id, kind, __ctox_kind: kind };
  return {
    ...doc,
    id,
    kind,
    title: safeText(doc.title) || safeText(doc.name) || safeText(doc.legalName) || safeText(doc.sourceName) || id,
    source_type: safeText(doc.source_type) || sourceTypeForRemote(remote),
    source_ref: safeText(doc.source_ref) || safeText(doc.sourceUrl) || id,
    status: safeText(doc.status) || (doc._deleted ? 'deleted' : 'active'),
    data,
    created_at_ms: Number(doc.created_at_ms || doc.createdAtMs || doc.updated_at_ms || now),
    updated_at_ms: Number(doc.updated_at_ms || now),
    _deleted: !!doc._deleted
  };
}

function safeText(value) {
  return typeof value === 'string' ? value.trim() : '';
}

function inferRemoteKind(remote) {
  if (remote === 'matching_objects') return 'object';
  if (remote === 'matching_results') return 'match';
  return 'requirement';
}

function sourceTypeForRemote(remote) {
  if (remote === 'matching_objects') return 'object';
  if (remote === 'matching_results') return 'match';
  return 'requirement';
}

async function saveDocument(name, input) {
  const now = new Date().toISOString();
  const doc = {
    ...toPlain(input),
    id: String(input?.id || makeId(name)),
    updatedAt: input?.updatedAt || now,
    updated_at_ms: Date.now()
  };
  const remote = COLLECTION_MAP[name] || name;
  const persisted = toPersistedDocument(name, doc);
  const raw = await getRawDatabase();
  const rxCollection = raw[remote];
  if (!rxCollection) throw new Error(`Matching collection unavailable: ${remote}`);
  if (typeof rxCollection.upsert === 'function') {
    await rxCollection.upsert(persisted);
  } else {
    const existing = await rxCollection.findOne(persisted.id).exec().catch(() => null);
    if (existing) await existing.incrementalPatch(persisted);
    else await rxCollection.insert(persisted);
  }

  invalidateCacheForRemote(remote);
  const docs = await loadCollection(name);
  const idx = docs.findIndex((item) => item.id === doc.id);
  if (idx >= 0) docs[idx] = doc;
  else docs.push(doc);
  cacheByCollection.set(name, docs);
  return doc;
}

function invalidateCacheForRemote(remote) {
  for (const [name, mapped] of Object.entries(COLLECTION_MAP)) {
    if (mapped === remote) cacheByCollection.delete(name);
  }
}

function wrapDoc(name, doc) {
  return {
    ...doc,
    toJSON: () => structuredCloneSafe(doc),
    atomicPatch: async (patch) => saveDocument(name, { ...doc, ...patch }),
    atomicUpdate: async (fn) => saveDocument(name, fn(structuredCloneSafe(doc))),
    incrementalModify: async (fn) => saveDocument(name, fn(structuredCloneSafe(doc))),
    update: async (patch) => saveDocument(name, { ...doc, ...(patch?.$set || patch || {}) }),
    remove: async () => saveDocument(name, { ...doc, _deleted: true })
  };
}

function queryDocuments(name, docs, query = {}) {
  const selector = query.selector || {};
  let result = docs.filter((doc) => matchesSelector(doc, selector));
  if (Array.isArray(query.sort) && query.sort.length) {
    result = [...result].sort((a, b) => compareBySort(a, b, query.sort));
  }
  if (Number.isFinite(query.limit)) result = result.slice(0, query.limit);
  return result;
}

function matchesSelector(doc, selector) {
  for (const [key, expected] of Object.entries(selector || {})) {
    const actual = getByPath(doc, key);
    if (expected && typeof expected === 'object' && !Array.isArray(expected)) {
      if ('$in' in expected && !expected.$in.map(String).includes(String(actual))) return false;
      if ('$eq' in expected && String(actual) !== String(expected.$eq)) return false;
      if ('$ne' in expected && String(actual) === String(expected.$ne)) return false;
      continue;
    }
    if (String(actual ?? '') !== String(expected ?? '')) return false;
  }
  return true;
}

function compareBySort(a, b, sortSpec) {
  for (const spec of sortSpec) {
    const [key, dir] = Object.entries(spec)[0] || [];
    const direction = String(dir || 'asc').toLowerCase() === 'desc' ? -1 : 1;
    const av = getByPath(a, key);
    const bv = getByPath(b, key);
    if (av === bv) continue;
    return String(av ?? '').localeCompare(String(bv ?? '')) * direction;
  }
  return 0;
}

function fromPersistedDocument(name, row) {
  const data = row?.data && typeof row.data === 'object' ? row.data : row;
  return { ...data, __ctox_kind: row?.kind || data?.__ctox_kind || inferKind(name) };
}

function toPersistedDocument(name, doc) {
  const now = Date.now();
  return {
    id: doc.id,
    kind: inferKind(name),
    title: titleFor(name, doc),
    source_type: sourceTypeFor(name),
    source_ref: doc.sourceUrl || doc.source_ref || doc.id,
    data: {
      ...doc,
      __ctox_kind: inferKind(name)
    },
    status: doc.status || 'active',
    created_at_ms: Number(doc.created_at_ms || doc.createdAtMs || now),
    updated_at_ms: now,
    _deleted: !!doc._deleted
  };
}

function belongsToUiCollection(name, doc) {
  const kind = doc.__ctox_kind || '';
  if (!kind) return true;
  if (name === 'sources') return kind === 'source';
  if (name === 'requirements') return kind === 'requirement';
  if (name === 'requirementSources') return kind === 'requirementSource' || kind === 'requirement';
  if (name === 'objects') return kind === 'object' || kind === 'object';
  if (name === 'matches') return kind === 'match' || kind === 'result';
  return true;
}

function inferKind(name) {
  if (name === 'sources') return 'source';
  if (name === 'requirements') return 'requirement';
  if (name === 'requirementSources') return 'requirementSource';
  if (name === 'objects') return 'object';
  if (name === 'matches') return 'match';
  return name;
}

function sourceTypeFor(name) {
  if (name === 'objects') return 'object';
  if (name === 'matches') return 'match';
  return 'requirement';
}

function titleFor(name, doc) {
  return doc.title || doc.name || doc.label || `${inferKind(name)} ${doc.id}`;
}

function toPlain(input) {
  if (input?.toJSON) return input.toJSON();
  return structuredCloneSafe(input || {});
}

function structuredCloneSafe(value) {
  if (typeof structuredClone === 'function') return structuredClone(value);
  return JSON.parse(JSON.stringify(value));
}

function getByPath(obj, path) {
  return String(path || '').split('.').reduce((acc, key) => (acc == null ? acc : acc[key]), obj);
}

function makeId(prefix) {
  if (globalThis.crypto?.randomUUID) return `${prefix}_${crypto.randomUUID()}`;
  return `${prefix}_${Date.now()}_${Math.random().toString(16).slice(2)}`;
}

async function loadRxdb() {
  const mod = await import('../../../vendor/rxdb-bundle.mjs');
  registerRxdbPlugin(mod, mod.RxDBMigrationSchemaPlugin || mod.RxDBMigrationPlugin);
  const rxdb = typeof mod.rxdbCore === 'function'
    ? mod.rxdbCore()
    : (globalThis.rxdbCore || mod);
  registerRxdbPlugin(rxdb, rxdb.RxDBMigrationSchemaPlugin || mod.RxDBMigrationSchemaPlugin || mod.RxDBMigrationPlugin);
  return rxdb;
}

function registerRxdbPlugin(target, plugin) {
  const add = target?.addRxPlugin;
  if (typeof add !== 'function' || !plugin) return;
  try {
    add(plugin);
  } catch (error) {
    const message = String(error?.message || error || '');
    if (!message.toLowerCase().includes('already')) throw error;
  }
}
