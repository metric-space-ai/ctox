import { collections as matchingSchemas, migrationStrategies as matchingMigrationStrategies } from '../schema.js';

const DB_NAME = 'ctox_business_os_v4';

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
let diagnosticsCache = null;

export function setBusinessOsRawDatabase(raw) {
  injectedRawDatabase = raw && typeof raw === 'object' ? raw : null;
  dbPromise = null;
  rawDbPromise = null;
  diagnosticsCache = null;
  cacheByCollection.clear();
  hydratedRemoteCollections.clear();
}

export async function getContactsCollection() {
  return { database: await getDatabase() };
}

export async function getMatchingCollectionDiagnostics({ probePull = false } = {}) {
  if (!probePull && diagnosticsCache) return structuredCloneSafe(diagnosticsCache);

  const collections = await Promise.all(
    ['matching_requirements', 'matching_objects', 'matching_results'].map(async (collectionName) => {
      const local = await describeLocalCollection(collectionName);
      const pull = probePull ? describeLocalPullEquivalent(collectionName, local) : null;
      return {
        collection: collectionName,
        schemaVersion: Number(matchingSchemas?.[collectionName]?.version ?? 0),
        localCount: local.count,
        localError: local.error,
        sync: describeRxdbSyncCollection(collectionName, local),
        pull
      };
    })
  );

  diagnosticsCache = {
    checkedAt: new Date().toISOString(),
    databaseName: DB_NAME,
    collections
  };
  return structuredCloneSafe(diagnosticsCache);
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
  if (injectedRawDatabase) {
    await ensureMatchingCollections(injectedRawDatabase);
    return injectedRawDatabase;
  }
  const rxdb = await loadRxdb();
  const { createRxDatabase, getCtoxIndexedDbStorage } = rxdb;
  const raw = await createRxDatabase({
    name: DB_NAME,
    storage: getCtoxIndexedDbStorage(),
    multiInstance: false,
    closeDuplicates: true,
  });
  await ensureMatchingCollections(raw);
  return raw;
}

async function ensureMatchingCollections(raw) {
  if (!raw || typeof raw !== 'object') return;
  const missing = {};
  for (const [collectionName, definition] of Object.entries(matchingSchemas || {})) {
    if (!raw[collectionName]) {
      missing[collectionName] = withMigrationStrategies(collectionName, normalizeCollectionDefinition(definition));
    }
  }
  if (Object.keys(missing).length) await raw.addCollections(missing);
}

function normalizeCollectionDefinition(definition) {
  if (definition?.schema) return definition;
  return { schema: definition };
}

function withMigrationStrategies(collectionName, definition) {
  const strategies = matchingMigrationStrategies?.[collectionName];
  return strategies ? { ...definition, migrationStrategies: strategies } : definition;
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
  void rxCollection;
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
  diagnosticsCache = null;
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
  const normalized = normalizePersistedDataForUi(name, data || {}, row || {});
  const kind = row?.kind || data?.__ctox_kind || inferKindFromData(name, data || {}) || inferKind(name);
  return {
    ...normalized,
    definitionId: normalized?.definitionId || data?.definitionId || row?.definition_id || '',
    schemaVersion: normalized?.schemaVersion || data?.schemaVersion || row?.schema_version || '',
    __ctox_kind: kind
  };
}

function normalizePersistedDataForUi(name, data, row) {
  if (!data || typeof data !== 'object') return {};

  if ((name === 'requirements' || name === 'requirementSources' || name === 'sources') && data.requirement && typeof data.requirement === 'object') {
    const requirement = data.requirement;
    const source = data.source && typeof data.source === 'object' ? data.source : {};
    const requirementSource = data.requirementSource && typeof data.requirementSource === 'object' ? data.requirementSource : {};
    return {
      ...data,
      ...requirement,
      id: requirement.id || data.id || row.id,
      title: requirement.title || data.title || row.title || '',
      sourceId: requirement.sourceId || requirement.source_id || source.id || data.sourceId || data.source_id || '',
      sourceName: requirement.sourceName || source.name || source.legalName || data.sourceName || '',
      sourceLogoUrl: source.logoUrl || source.logo_url || '',
      parsed: requirementSource.parsed || data.parsed || null,
      rawText: requirementSource.rawText || requirementSource.raw_text || data.rawText || ''
    };
  }

  if (name === 'objects' && data.object && typeof data.object === 'object') {
    const object = data.object;
    return {
      ...data,
      ...object,
      id: object.id || data.id || row.id,
      name: object.name || data.name || row.title || '',
      taxonomy: object.taxonomy || object.currentRole || object.desiredPosition || data.taxonomy || '',
      skills: object.skills || data.skills || [],
      languages: object.languages || data.languages || [],
      education: object.education || data.education || [],
      experience: object.experience || data.experience || [],
      executiveInfo: object.executiveInfo || data.executiveInfo || {}
    };
  }

  if (name === 'matches' && data.match && typeof data.match === 'object') {
    const match = data.match;
    const requirement = data.requirement && typeof data.requirement === 'object' ? data.requirement : {};
    const object = data.object && typeof data.object === 'object' ? data.object : {};
    const source = data.source && typeof data.source === 'object' ? data.source : {};
    return {
      ...data,
      ...match,
      id: match.id || data.id || row.id,
      sourceId: match.sourceId || match.source_id || source.id || requirement.sourceId || requirement.source_id || '',
      requirementId: match.requirementId || match.requirement_id || requirement.id || '',
      objectId: match.objectId || match.object_id || object.id || '',
      items: match.items || data.items || data.evidence || [],
      score: match.score ?? data.score
    };
  }

  return data;
}

function inferKindFromData(name, data) {
  if (!data || typeof data !== 'object') return '';
  if (data.__ctox_kind) return data.__ctox_kind;
  if (data.requirement) return 'requirement';
  if (data.object) return 'object';
  if (data.match) return 'match';
  if (name === 'matches' && (data.requirementId || data.requirement_id || data.objectId || data.object_id)) return 'match';
  if (name === 'objects' && (data.name || data.firstName || data.lastName || data.currentRole)) return 'object';
  if ((name === 'requirements' || name === 'requirementSources') && (data.title || data.sourceId || data.source_id)) return 'requirement';
  return '';
}

function toPersistedDocument(name, doc) {
  const now = Date.now();
  return {
    id: doc.id,
    kind: inferKind(name),
    title: titleFor(name, doc),
    source_type: sourceTypeFor(name),
    source_ref: doc.sourceUrl || doc.source_ref || doc.id,
    definition_id: doc.definitionId || '',
    schema_version: doc.schemaVersion || '',
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

async function describeLocalCollection(collectionName) {
  try {
    const raw = await getRawDatabase();
    const rxCollection = raw?.[collectionName];
    if (!rxCollection?.find) return { count: 0, error: 'collection unavailable' };
    const rows = await rxCollection.find().exec();
    return { count: Array.isArray(rows) ? rows.length : 0, error: '' };
  } catch (error) {
    return { count: 0, error: String(error?.message || error || 'unknown error') };
  }
}

function describeRxdbSyncCollection(collectionName, local) {
  return {
    ok: !local.error,
    mode: 'rxdb-webrtc',
    count: Number(local.count || 0),
    error: local.error || ''
  };
}

function describeLocalPullEquivalent(collectionName, local) {
  return {
    ok: !local.error,
    status: 'rxdb-local',
    collection: collectionName,
    count: Number(local.count || 0),
    error: local.error || ''
  };
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
  const mod = await import('../../../rxdb/dist/ctox-rxdb-js.mjs?v=20260607-outbound-rxdb-main1');
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
