export const COLLECTION = '__COLLECTION__';

export function normalizeRecord(input = {}, options = {}) {
  const nowMs = Number(options.nowMs || Date.now());
  const title = String(input.title || 'Untitled record').trim() || 'Untitled record';
  return {
    id: String(input.id || `${title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'record'}-${nowMs}`).slice(0, 180),
    title,
    status: String(input.status || 'open'),
    notes: String(input.notes || ''),
    is_deleted: Boolean(input.is_deleted),
    created_at_ms: Number(input.created_at_ms || nowMs),
    updated_at_ms: Number(input.updated_at_ms || nowMs)
  };
}

export function visibleRecords(records = [], query = '', status = '') {
  const needle = String(query).trim().toLowerCase();
  return records
    .filter((record) => !record.is_deleted)
    .filter((record) => !status || record.status === status)
    .filter((record) => !needle || `${record.title} ${record.notes}`.toLowerCase().includes(needle));
}
