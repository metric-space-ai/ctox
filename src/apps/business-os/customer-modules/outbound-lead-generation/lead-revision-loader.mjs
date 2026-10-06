// Read-only membership/revision projections never become full Lead records.
// All full-data consumers retain the unprojected rows returned by this loader.
let readSequence = 0;
const docData = (doc) => doc?.toJSON?.() || doc;
function loadError(message, code, retryable = false) {
  return Object.assign(new Error(message), { code, retryable });
}
async function membership(collection, token, pageSize) {
  const result = new Map();
  let cursor = '';
  for (let page = 0; page < 100; page++) {
    const docs = await collection.find({
      selector: cursor ? { id: { $gt: cursor } } : {},
      sort: [{ id: 'asc' }], limit: pageSize,
      projection: ['id'], requireRevision: token + ':' + page,
    }).exec();
    if (!docs.length) return result;
    let next = cursor;
    for (const doc of docs) {
      const row = docData(doc);
      if (typeof row?.id !== 'string' || !row.id || typeof row._rev !== 'string' || !row._rev) {
        throw loadError('Lead revision projection did not include id/_rev', 'LEAD_REVISION_MISSING');
      }
      if (row.id <= cursor || result.has(row.id)) {
        throw loadError('Lead revision pagination did not advance', 'LEAD_REVISION_PAGINATION');
      }
      if (row.id > next) next = row.id;
      if (!row._deleted) result.set(row.id, row._rev);
    }
    cursor = next;
  }
  throw loadError('Lead revision pagination exceeded 100 pages', 'LEAD_REVISION_PAGE_LIMIT');
}
export async function loadLeadRevisionChanges(collection, previousRows, { pageSize = 200, batchSize = 8 } = {}) {
  const previous = new Map(previousRows.map((row) => [row.id, row]));
  // A strict read token is an operation identity, not an authorization or a
  // document revision. The Shell binds it to its current bridge generation.
  const readToken = 'outbound-leads:' + ++readSequence + ':' + (globalThis.crypto?.randomUUID?.() || Math.random().toString(36).slice(2));
  for (let attempt = 0; attempt < 2; attempt++) {
    const revisions = await membership(collection, readToken + ':' + attempt, pageSize);
    const changedIds = new Set([...revisions].filter(([id, rev]) => previous.get(id)?._rev !== rev).map(([id]) => id));
    const removedIds = new Set([...previous.keys()].filter((id) => !revisions.has(id)));
    const hydrated = new Map();
    const ids = [...changedIds];
    for (let offset = 0; offset < ids.length; offset += batchSize) {
      const batch = ids.slice(offset, offset + batchSize);
      const expected = new Map(batch.map((id) => [id, revisions.get(id)]));
      const docs = await collection.find({
        selector: { id: { $in: batch } }, sort: [{ id: 'asc' }], limit: batch.length,
        // Full hydration cannot reuse a projected window or a previous bridge.
        requireRevision: readToken + ':full:' + attempt + ':' + offset,
      }).exec();
      for (const doc of docs) {
        const row = docData(doc);
        if (!expected.has(row?.id) || hydrated.has(row.id)) {
          throw loadError('Unexpected Lead returned by full hydration', 'LEAD_HYDRATION_UNEXPECTED');
        }
        hydrated.set(row.id, row);
      }
    }
    const raced = [...changedIds].some((id) => (
      hydrated.get(id)?._rev !== revisions.get(id) || hydrated.get(id)?._deleted
    ));
    if (raced) continue; // A concurrent update/delete is checked against a fresh manifest once.
    return {
      rows: [...revisions.keys()].map((id) => hydrated.get(id) || previous.get(id)),
      changedIds, removedIds,
    };
  }
  // Preserve the prior App state; the collection reloader retries with backoff.
  throw loadError('Lead changed during revision hydration; retry required', 'LEAD_HYDRATION_RACE', true);
}
