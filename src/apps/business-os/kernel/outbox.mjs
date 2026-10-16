import {checkCommandTransition} from './command-state.mjs';
import { cloneValue, mergePatch } from './values.mjs';
// ---------------------------------------------------------------------------------------------------------------
export const emptyBase = () => ({ data: null, deleted: false, rev: null, seq: '0' });
export const byOpId = (a, b) => {
  if ((a.client_order == null) !== (b.client_order == null)) return a.client_order == null ? -1 : 1;
  if (a.client_order != null && b.client_order != null) {
    const x = BigInt(a.client_order), y = BigInt(b.client_order);
    return x < y ? -1 : x > y ? 1 : 0;
  }
  return a.op_id < b.op_id ? -1 : a.op_id > b.op_id ? 1 : 0;
};

// The shared identity database orders submissions, never the tab's clock.
export async function enqueueWrite(t, entry) {
  const order = String(BigInt((await t.get('meta', 'outbox_order'))?.value ?? '0') + 1n);
  await t.put('meta', { key: 'outbox_order', value: order });
  entry.client_order = order;
  entry.client_seq = null; // cancellable until the leader durably reserves dispatch
  await t.put('outbox', entry);
}

// Reserve the WHOLE ordered batch, including later edits to the same document.
// Numbering only ready document heads lets a command overtake an earlier edit.
// Caller checks leader_gen in this transaction. Reserved writes cannot cancel:
// deletion would leave an unfillable gap at the core after reconnect.
export async function reserveWrites(t) {
  const entries = (await t.all('outbox')).sort(byOpId);
  let seq = BigInt((await t.get('meta', 'client_seq'))?.value ?? '0');
  for (const entry of entries) if (entry.client_seq != null && BigInt(entry.client_seq) > seq) seq = BigInt(entry.client_seq);
  for (const entry of entries) {
    if (entry.op === 'upload' || entry.client_seq != null) continue;
    entry.client_seq = String(++seq);
    await t.put('outbox', entry);
  }
  await t.put('meta', { key: 'client_seq', value: String(seq) });
  return entries;
}
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
  const mine = outboxEntries.filter((e) => e.collection === ns && e.id === id && e.op !== 'upload' && !e.awaiting_page).sort(byOpId);
  for (const e of mine) {
    // A queued optimistic intent must not mask its own authoritative progress
    // after a result ACK is lost. Keep the outbox for exact replay, but expose
    // the matching Core status immediately; a different intent cannot borrow it.
    if (/^business_commands(?:@g[1-9][0-9]*)?$/.test(ns) && ['accepted','running','done','failed','cancelled','rejected'].includes(st.data?.status) && e.op === 'put' && e.body?.status === 'queued'
        && !checkCommandTransition({...st.data,status:'queued'},e.body)) continue;
    st = applyOp(st, e);
  }
  const last = mine[mine.length - 1];
  return {
    collection: ns, id, data: st.data, deleted: st.deleted,
    rev: last ? `op:${last.op_id}` : base.rev, // §16.6: optimistische lokale Revision
    seq: base.seq, pending: mine.length > 0, base,
  };
}
