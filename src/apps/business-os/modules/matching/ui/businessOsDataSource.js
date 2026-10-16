const MATCHING_REMOTE_COLLECTIONS = Object.freeze([
  'matching_requirements',
  'matching_objects',
  'matching_results'
]);

const COLLECTION_MAP = {
  sources: 'matching_requirements',
  requirements: 'matching_requirements',
  requirementSources: 'matching_requirements',
  objects: 'matching_objects',
  matches: 'matching_results',
  object_photo_chunks: 'matching_objects',
  object_photo_variant_chunks: 'matching_objects'
};

// App-API 4.0 (V4-PORT.md): je Montage ein Live-Abo je Collection. Lesen kommt aus dem lokalen
// Live-Stand (api.query), Schreiben über api.put/api.remove. Keine Readiness, kein Nachladen.
let matchingApi = null;
let dbPromise = null;
const liveRowsByRemote = new Map();
const liveUnsubscribers = [];
const changeListeners = new Map();

export function setMatchingDataApi(api) {
  disposeMatchingData();
  matchingApi = api || null;
  dbPromise = null;
  if (!matchingApi) return;
  for (const remote of MATCHING_REMOTE_COLLECTIONS) {
    let previous = null;
    const off = matchingApi
      .query(remote, { sort: [{ updated_at_ms: 'desc' }] })
      .subscribe((rows) => {
        const next = new Map(rows.map((row) => [row.id, row]));
        liveRowsByRemote.set(remote, rows);
        const first = previous === null;
        const changed = first ? [] : changedIds(previous, next);
        previous = next;
        if (!first) emitChange(remote, changed);
      });
    liveUnsubscribers.push(off);
  }
}

export function disposeMatchingData() {
  while (liveUnsubscribers.length) {
    try { liveUnsubscribers.pop()?.(); } catch (_) {}
  }
  liveRowsByRemote.clear();
  changeListeners.clear();
  matchingApi = null;
  dbPromise = null;
}

function changedIds(previous, next) {
  const ids = [];
  for (const [id, row] of next) {
    const before = previous.get(id);
    if (!before || before !== row && (before.updated_at_ms !== row.updated_at_ms || before.status !== row.status || before.title !== row.title)) ids.push(id);
  }
  for (const id of previous.keys()) if (!next.has(id)) ids.push(id);
  return ids;
}

function emitChange(remote, ids) {
  for (const [name, listeners] of changeListeners) {
    if ((COLLECTION_MAP[name] || name) !== remote) continue;
    const events = ids.length && ids.length <= 25 ? ids.map((id) => ({ documentData: { id } })) : [{}];
    for (const listener of [...listeners]) {
      for (const event of events) {
        try { listener(event); } catch (error) { console.warn('[matching] change listener failed', error); }
      }
    }
  }
}

export async function getContactsCollection() {
  return { database: await getDatabase() };
}

export async function getDatabase() {
  if (!dbPromise) dbPromise = Promise.resolve(createDatabase());
  return dbPromise;
}

function createDatabase() {
  const db = {};
  for (const name of Object.keys(COLLECTION_MAP)) {
    db[name] = createCollection(name);
  }
  return db;
}

// Fassade der UI: all(query) / one(selector|id) / save(doc) / onChange(cb). Ergebnisse sind Klone
// mit modify/remove; Lesen ist synchron aus dem Live-Stand (als Promise, wie die UI es erwartet).
function createCollection(name) {
  return {
    onChange(handler) {
      if (typeof handler !== 'function') return () => {};
      if (!changeListeners.has(name)) changeListeners.set(name, new Set());
      changeListeners.get(name).add(handler);
      return () => changeListeners.get(name)?.delete(handler);
    },
    async all(query = {}) {
      return queryDocuments(name, uiDocuments(name), query).map((doc) => wrapDoc(name, doc));
    },
    async one(arg = {}) {
      const selector = typeof arg === 'string' ? { id: arg } : (arg.selector || arg || {});
      const found = queryDocuments(name, uiDocuments(name), { selector })[0] || null;
      return found ? wrapDoc(name, found) : null;
    },
    save: async (doc) => wrapDoc(name, await saveDocument(name, doc))
  };
}

function uiDocuments(name) {
  const remote = COLLECTION_MAP[name] || name;
  const rows = liveRowsByRemote.get(remote) || [];
  return rows
    .map((row) => fromPersistedDocument(name, structuredCloneSafe(row)))
    .filter((doc) => !doc?._deleted && belongsToUiCollection(name, doc));
}

async function saveDocument(name, input) {
  const now = new Date().toISOString();
  const doc = {
    ...toPlain(input),
    id: String(input?.id || makeId(name)),
    updatedAt: input?.updatedAt || now,
    updated_at_ms: Date.now()
  };
  const remote = matchingCollectionName(COLLECTION_MAP[name] || name);
  if (!matchingApi) throw new Error(`Matching collection missing: ${remote}`);
  if (doc._deleted) {
    const handle = matchingApi.remove(remote, doc.id);
    await handle.local;
    return doc;
  }
  const persisted = toPersistedDocument(name, doc);
  delete persisted._deleted;
  const handle = matchingApi.put(remote, persisted);
  await handle.local;
  return doc;
}

function matchingCollectionName(collectionName) {
  const name = String(collectionName || '').trim();
  if (!name || !MATCHING_REMOTE_COLLECTIONS.includes(name)) {
    throw new Error(`Matching collection is not part of the module contract: ${name || 'unknown'}`);
  }
  return name;
}

function wrapDoc(name, doc) {
  return {
    ...doc,
    toJSON: () => structuredCloneSafe(doc),
    atomicPatch: async (patch) => saveDocument(name, { ...doc, ...patch }),
    atomicUpdate: async (fn) => saveDocument(name, fn(structuredCloneSafe(doc))),
    modify: async (fn) => saveDocument(name, fn(structuredCloneSafe(doc))),
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
