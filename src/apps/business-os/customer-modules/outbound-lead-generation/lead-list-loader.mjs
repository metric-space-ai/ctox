// List DTOs have their own store. They are never evidence, export or write inputs.
export const LEAD_LIST_PROJECTION = Object.freeze([
  'id', 'name', 'domain', 'website', 'city', 'country', 'campaign',
  'research_status', 'validation_status', 'sellify_status', 'updated_at_ms',
  'research_updated_at_ms', 'command_id', 'task_id',
  'payload.weitere_kampagnen', 'payload.campaign_task_id',
  'payload.campaign_command_id', 'payload.research_execution_phase',
  'payload.research_execution_phase_command_id', 'payload.research_execution_detail',
  'payload.research_queued_at_ms', 'payload.sellify_precheck.known', 'payload.sellify_precheck.contact_id',
  'payload.sellify_started_at_ms', 'payload.imported_row.sellify_contact_id',
]);
const envelopeFields = new Set(['_rev', '_meta', '_deleted', '_attachments']);
function projectionContainsOnlyListFields(row, paths = LEAD_LIST_PROJECTION, root = true) {
  if (!row || typeof row !== 'object' || Array.isArray(row)) return false;
  return Object.keys(row).every(key => {
    if (root && envelopeFields.has(key)) return true;
    if (paths.includes(key)) return true;
    const nested = paths.filter(path => path.startsWith(key + '.')).map(path => path.slice(key.length + 1));
    return nested.length > 0 && projectionContainsOnlyListFields(row[key], nested, false);
  });
}
let sequence = 0;
const json = (doc) => doc?.toJSON?.() || doc;
const token = () => `outbound-list:${++sequence}:${globalThis.crypto?.randomUUID?.() || Math.random()}`;
const error = (message, code) => Object.assign(new Error(message), { code, retryable: true });

export function leadListRow(full) {
  const result = { _rev: full._rev };
  for (const path of LEAD_LIST_PROJECTION) {
    const parts = path.split('.');
    let value = full;
    for (const key of parts) value = value?.[key];
    if (value === undefined) continue;
    let target = result;
    for (const key of parts.slice(0, -1)) target = (target[key] ||= {});
    target[parts.at(-1)] = structuredClone(value);
  }
  return result;
}

// Client first: once the replicated store holds the whole collection
// (localReplicaComplete, set when the pull drained; after a reload it resumes
// from the persisted checkpoint), every read is answered from the browser.
// Native reads remain only for the first sync of a fresh profile.
export function hasLocalStore(collection) {
  return typeof collection?.storageCollection?.allDocuments === 'function';
}

export function readsLocally(collection) {
  return collection?.localReplicaComplete === true;
}

// Rows already stored in the browser, read straight from the local store. The
// shell routes find().exec() to CTOX while localReplicaComplete is false; after
// every reload or native restart that lasted until the pull drained, and a lead
// detail hung on "werden geladen" although the lead was stored locally
// (Kundeninstanz, 15.10.2026). Missing rows stay absent; callers fetch only those.
export async function readStoredLeadRows(collection, ids) {
  const store = collection?.storageCollection;
  if (typeof store?.findDocumentsById !== 'function') return new Map();
  const found = await store.findDocumentsById([...new Set(ids.filter(id => typeof id === 'string' && id))]);
  return new Map(Object.entries(found || {})
    .filter(([id, row]) => row && row.id === id && !row._deleted));
}

export async function loadLeadList(collection, previousRows = [], { pageSize = 200, signal } = {}) {
  const previous = new Map(previousRows.map(row => [row.id, row]));
  // Client first: the list always comes from the browser's own store when one
  // exists. Through find().exec() it went to CTOX until the shell marked the
  // replica complete; after a reload or native restart that read timed out,
  // the app stayed on its startup snapshot and blocked every action
  // (Kundeninstanz, 15.10.2026: deleting a source did nothing).
  if (hasLocalStore(collection)) {
    const rows = [];
    for (const full of await collection.storageCollection.allDocuments()) {
      if (!full?.id || full._deleted) continue;
      rows.push(leadListRow(full));
    }
    rows.sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
    const ids = new Set(rows.map(row => row.id));
    return {
      rows,
      changedIds: new Set(rows.filter(row => JSON.stringify(previous.get(row.id) || null) !== JSON.stringify(row)).map(row => row.id)),
      removedIds: new Set([...previous.keys()].filter(id => !ids.has(id))),
    };
  }
  if (readsLocally(collection)) {
    // Local reads are windowed (at most 200 rows per query): page by id.
    const rows = [];
    let after = '';
    for (let page = 0; page < 1000; page++) {
      const docs = await collection.find({
        selector: after ? { id: { $gt: after } } : {}, sort: [{ id: 'asc' }], limit: pageSize,
      }).exec({ signal });
      if (!docs.length) break;
      for (const doc of docs) {
        const full = json(doc);
        if (!full?.id) continue;
        after = full.id;
        if (full._deleted) continue;
        const before = previous.get(full.id);
        rows.push(before?._rev === full._rev ? before : leadListRow(full));
      }
      if (docs.length < pageSize) break;
    }
    const ids = new Set(rows.map(row => row.id));
    return {
      rows,
      changedIds: new Set(rows.filter(row => previous.get(row.id)?._rev !== row._rev).map(row => row.id)),
      removedIds: new Set([...previous.keys()].filter(id => !ids.has(id))),
    };
  }
  const rows = [];
  const seen = new Set();
  let cursor = '';
  const read = token();
  for (let page = 0; page < 100; page++) {
    const docs = await collection.find({
      selector: cursor ? { id: { $gt: cursor } } : {},
      sort: [{ id: 'asc' }], limit: pageSize,
      projection: [...LEAD_LIST_PROJECTION], requireRevision: `${read}:${page}`,
    }).exec({ signal });
    if (!docs.length) {
      const changedIds = new Set(rows.filter(row => previous.get(row.id)?._rev !== row._rev).map(row => row.id));
      return { rows, changedIds, removedIds: new Set([...previous.keys()].filter(id => !seen.has(id))) };
    }
    for (const doc of docs) {
      const row = json(doc);
      if (typeof row?.id !== 'string' || !row.id || typeof row._rev !== 'string' || !row._rev) {
        throw error('Die Lead-Liste enthält keine gültige Revision.', 'LEAD_LIST_REVISION_MISSING');
      }
      if (row.id <= cursor || seen.has(row.id)) throw error('Die Lead-Liste konnte nicht vollständig geladen werden.', 'LEAD_LIST_PAGINATION');
      cursor = row.id;
      if (row._deleted) continue;
      seen.add(row.id);
      // Reject a broken projection instead of silently materializing huge/full rows.
      if (!projectionContainsOnlyListFields(row)) {
        throw error('CTOX hat die Listenprojektion nicht angewendet.', 'LEAD_LIST_PROJECTION_NOT_APPLIED');
      }
      rows.push(previous.get(row.id)?._rev === row._rev ? previous.get(row.id) : row);
    }
  }
  throw error('Die Lead-Liste überschreitet das Seitenlimit.', 'LEAD_LIST_PAGE_LIMIT');
}

export async function loadFullLeadRows(collection, ids, { batchSize = 8, signal } = {}) {
  const requested = [...new Set(ids.filter(id => typeof id === 'string' && id))];
  const rows = [];
  if (readsLocally(collection)) {
    const docs = await collection.find({ selector: { id: { $in: requested } } }).exec({ signal });
    const found = new Map(docs.map(doc => json(doc)).filter(row => row?.id && !row._deleted).map(row => [row.id, row]));
    if (found.size !== requested.length) throw error('Mindestens ein Lead wurde gelöscht oder ist nicht mehr zugänglich.', 'LEAD_DETAIL_MISSING');
    return requested.map(id => found.get(id));
  }
  const read = token();
  for (let offset = 0; offset < requested.length; offset += batchSize) {
    const batch = requested.slice(offset, offset + batchSize);
    const docs = await collection.find({
      selector: { id: { $in: batch } }, sort: [{ id: 'asc' }], limit: batch.length,
      requireRevision: `${read}:full:${offset}`,
    }).exec({ signal });
    const found = new Set();
    for (const doc of docs) {
      const row = json(doc);
      if (!batch.includes(row?.id) || found.has(row.id) || row._deleted || typeof row._rev !== 'string' || !row._rev) {
        throw error('Der vollständige Lead konnte nicht eindeutig geladen werden.', 'LEAD_DETAIL_INVALID');
      }
      found.add(row.id); rows.push(row);
    }
    if (found.size !== batch.length) throw error('Mindestens ein Lead wurde gelöscht oder ist nicht mehr zugänglich.', 'LEAD_DETAIL_MISSING');
  }
  // All requested full rows must arrive before callers can publish or act.
  return rows;
}

/** Acquire the Shell's native query authority, never a follower/local fallback. */
export async function withLeadQueryAuthority(sync, work, { timeoutMs = 15000, isCurrent = () => true,
  collectionName = 'outbound_lead_generation_leads', collection = null } = {}) {
  // A complete local replica needs no native lease: leasing and releasing an
  // eager collection on every read stopped its replication on release.
  if (readsLocally(collection)) return work(new AbortController().signal);
  if (typeof sync?.leaseCollection !== 'function') throw error('CTOX stellt keinen aktuellen Lesekanal bereit.', 'LEAD_QUERY_AUTHORITY_MISSING');
  const deadline = Date.now() + timeoutMs;
  const controller = new AbortController();
  let closed = false, lease = null, timer = null;
  const acquisition = Promise.resolve().then(() => sync.leaseCollection(
    collectionName, 'outbound-collection-query', { forceDirect: true },
  ));
  // A timed-out acquisition may settle later; it still belongs to this caller.
  void acquisition.then(late => {
    if (closed) return late?.release?.();
  }).catch(() => {});
  const run = async () => {
    lease = await acquisition;
    let bridge = lease?.bridge;
    if (!bridge?.state && bridge?.ready) bridge = await bridge.ready;
    const replication = bridge?.state;
    if (typeof replication?.awaitQueryReady !== 'function') {
      throw error('Der aktuelle CTOX-Kanal ist noch nicht für Datenabfragen bereit.', 'LEAD_QUERY_AUTHORITY_MISSING');
    }
    await replication.awaitQueryReady(Math.max(1, deadline - Date.now()));
    const generation = replication.collectionQueryGenerationToken?.(replication.activeRemotePeerId);
    const assertCurrent = () => {
      if (closed || !isCurrent() || replication.cancelled || lease.bridge?.state !== replication
        || !generation || replication.collectionQueryGenerationToken?.(replication.activeRemotePeerId) !== generation) {
        throw error('Die CTOX-Verbindung hat sich während des Ladens geändert.', 'LEAD_QUERY_GENERATION_CHANGED');
      }
    };
    assertCurrent();
    const result = await work(controller.signal);
    assertCurrent();
    return result;
  };
  try {
    return await Promise.race([
      run(),
      new Promise((_, reject) => {
        timer = setTimeout(() => {
          controller.abort();
          reject(error('Daten konnten nicht rechtzeitig aus CTOX geladen werden.', 'LEAD_QUERY_TIMEOUT'));
        }, Math.max(1, deadline - Date.now()));
      }),
    ]);
  } finally {
    closed = true;
    if (timer !== null) clearTimeout(timer);
    controller.abort();
    if (lease) await lease.release().catch(() => {});
  }
}
