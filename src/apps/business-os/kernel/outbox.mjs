import { cloneValue, mergePatch } from './values.mjs';
// ---------------------------------------------------------------------------------------------------------------
export const emptyBase = () => ({ data: null, deleted: false, rev: null, seq: '0' });
export const byOpId = (a, b) => (a.op_id < b.op_id ? -1 : a.op_id > b.op_id ? 1 : 0);
export const docKey = (ns, id) => `${ns}\u0000${id}`;
export const nsName = (collection, gen) => (gen > 1 ? `${collection}@g${gen}` : collection);

export function applyOp(state, entry) {
  if (entry.op === 'put') return { data: cloneValue(entry.body), deleted: false };
  if (entry.op === 'patch') {
    const cur = state.data && !state.deleted ? state.data : { id: entry.id };
    const next = mergePatch(cur, entry.body);
    next.id = entry.id;
    return { data: next, deleted: false };
  }
  if (entry.op === 'delete') return { data: null, deleted: true };
  return state;
}
export function recompute(ns, id, base, outboxEntries) {
  let st = { data: base.data, deleted: base.deleted };
  const mine = outboxEntries.filter((e) => e.collection === ns && e.id === id && e.op !== 'upload').sort(byOpId);
  for (const e of mine) st = applyOp(st, e);
  const last = mine[mine.length - 1];
  return {
    collection: ns, id, data: st.data, deleted: st.deleted,
    rev: last ? `op:${last.op_id}` : base.rev, // §16.6: optimistische lokale Revision
    seq: base.seq, pending: mine.length > 0, base,
  };
}
