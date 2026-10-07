// List DTOs have their own store. They are never evidence, export or write inputs.
export const LEAD_LIST_PROJECTION = Object.freeze([
  'id', 'name', 'domain', 'website', 'city', 'country', 'campaign',
  'research_status', 'validation_status', 'sellify_status', 'updated_at_ms',
  'research_updated_at_ms', 'command_id', 'task_id',
  'payload.weitere_kampagnen', 'payload.campaign_task_id',
  'payload.campaign_command_id', 'payload.research_execution_phase',
  'payload.research_execution_phase_command_id', 'payload.research_execution_detail',
  'payload.research_queued_at_ms', 'payload.sellify_precheck',
]);
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

export async function loadLeadList(collection, previousRows = [], { pageSize = 200 } = {}) {
  const previous = new Map(previousRows.map(row => [row.id, row]));
  const rows = [];
  const seen = new Set();
  let cursor = '';
  const read = token();
  for (let page = 0; page < 100; page++) {
    const docs = await collection.find({
      selector: cursor ? { id: { $gt: cursor } } : {},
      sort: [{ id: 'asc' }], limit: pageSize,
      projection: [...LEAD_LIST_PROJECTION], requireRevision: `${read}:${page}`,
    }).exec();
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
      if (['data', 'contacts', 'evidence', 'field_status', 'person_field_status'].some(key => Object.hasOwn(row, key))) {
        throw error('CTOX hat die Listenprojektion nicht angewendet.', 'LEAD_LIST_PROJECTION_NOT_APPLIED');
      }
      rows.push(previous.get(row.id)?._rev === row._rev ? previous.get(row.id) : row);
    }
  }
  throw error('Die Lead-Liste überschreitet das Seitenlimit.', 'LEAD_LIST_PAGE_LIMIT');
}

export async function loadFullLeadRows(collection, ids, { batchSize = 8 } = {}) {
  const requested = [...new Set(ids.filter(id => typeof id === 'string' && id))];
  const rows = [];
  const read = token();
  for (let offset = 0; offset < requested.length; offset += batchSize) {
    const batch = requested.slice(offset, offset + batchSize);
    const docs = await collection.find({
      selector: { id: { $in: batch } }, sort: [{ id: 'asc' }], limit: batch.length,
      requireRevision: `${read}:full:${offset}`,
    }).exec();
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
