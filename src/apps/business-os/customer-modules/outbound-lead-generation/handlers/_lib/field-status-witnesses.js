// Feld-Status-Zeugen als OLG-Collection (Strang OLGSCR).
//
// Vorher: SQLite-Tabelle outbound_native_field_status_witnesses im Core hinter
// den Core-Fähigkeiten „Feldstatus-Zeugen lesen/ausstellen“. Jetzt: ein Dokument je Lead
// in outbound_lead_generation_field_status_witnesses {id: record_id, witnesses:[…]}.
// Die Ausstellung läuft in derselben ctx.transaction wie das Lead-Schreiben – genau wie
// Rust (store.rs:12753 upsert_native_field_status_if_current: eine SQLite-Transaktion).
//
// ref: outbound_field_review.rs:26-79 stamped / field_status_snapshot,
//      :139-166 NativeFieldStatusWitnesses::load, :168-175 retain_current,
//      :218-276 persist (Issuance::Writeback), :406 valid_id,
//      store.rs:12652-12715 load_current_native_field_status_snapshot.
import { has, isObject } from './olg.js';
import { jsonEqual } from './research-source-adapter.js';
import { validId } from './research-field-status.js';

export const WITNESSES = 'outbound_lead_generation_field_status_witnesses';

const get = (value, key) => (has(value, key) ? value[key] : undefined);
const clone = (value) => (value === undefined ? undefined : JSON.parse(JSON.stringify(value)));
const keyOf = (scope, person, field) => `${scope}\u0000${person}\u0000${field}`;

/** ref: :26 stamped — `revision` or `review` present and not null. */
export function stamped(status) {
  return ['revision', 'review'].some((key) => has(status, key) && status[key] !== null);
}

/**
 * ref: :31-79 field_status_snapshot → Map(key → {scope, person_key, field, status}) or null
 * (ambiguous identity: stamped person statuses under an invalid key, or a stamped contact
 * without a unique valid person_key).
 */
export function fieldStatusSnapshot(lead) {
  const out = new Map();
  const fields = (scope, person, statuses) => {
    if (!isObject(statuses)) return;
    for (const [field, status] of Object.entries(statuses)) {
      if (stamped(status)) out.set(keyOf(scope, person, field), { scope, person_key: person, field, status: clone(status) });
    }
  };
  fields('lead', '', get(lead, 'field_status'));
  const persons = get(lead, 'person_field_status');
  if (isObject(persons)) {
    for (const [person, statuses] of Object.entries(persons)) {
      if (isObject(statuses) && Object.values(statuses).some(stamped) && !validId(person)) return null;
      fields('person', person, statuses);
    }
  }
  const contacts = Array.isArray(get(lead, 'contacts')) ? lead.contacts : [];
  const people = new Map();
  for (const contact of contacts) {
    const key = get(contact, 'person_key');
    if (typeof key === 'string') people.set(key, (people.get(key) ?? 0) + 1);
  }
  for (const contact of contacts) {
    const statuses = get(contact, 'field_status');
    if (!(isObject(statuses) && Object.values(statuses).some(stamped))) continue;
    const person = get(contact, 'person_key');
    if (!(typeof person === 'string' && validId(person))) return null;
    // An unmarked duplicate must not borrow the stamped one's identity.
    if (people.get(person) !== 1) return null;
    fields('contact', person, statuses);
  }
  return out;
}

/** ref: :139 load — stored witnesses of one record ([] without document). */
export async function loadWitnesses(store, recordId) {
  const doc = await store.get(WITNESSES, recordId);
  return Array.isArray(get(doc, 'witnesses')) ? doc.witnesses.filter(isObject).map(clone) : [];
}

/** ref: :168 retain_current — keep only witnesses whose status equals the lead's current status. */
export function retainCurrent(witnesses, lead) {
  const current = fieldStatusSnapshot(lead);
  if (current == null) return [];
  return witnesses.filter((w) => {
    const c = current.get(keyOf(w.scope, w.person_key, w.field));
    return c !== undefined && jsonEqual(c.status, w.status);
  });
}

/**
 * ref: store.rs:12652 load_current_native_field_status_witnesses — witnesses read together
 * with the current lead, reduced to those that still certify the current status.
 * Ersetzt die frühere Core-Fähigkeit „Zeugen lesen“.
 */
export async function readCurrentWitnesses(ctx, leadCollection, recordId) {
  return ctx.transaction(async (tx) => {
    const lead = await tx.get(leadCollection, recordId);
    if (lead == null || lead._deleted === true || lead.is_deleted === true) return [];
    return retainCurrent(await loadWitnesses(tx, recordId), lead);
  });
}

/**
 * ref: :218-276 persist with NativeFieldStatusIssuance::Writeback — inside the caller's
 * transaction. A status stays witnessed if it was witnessed with exactly that value, or is
 * newly issued: its key is in `keys`, its `revision` equals this writeback revision and it
 * carries no review. Throws on an ambiguous identity (Rust: context error).
 * Ersetzt die frühere Core-Fähigkeit „Zeugen ausstellen“.
 */
export async function persistWritebackWitnesses(tx, recordId, lead, previous, revision, keys, nowMs) {
  const current = fieldStatusSnapshot(lead);
  if (current == null) {
    const error = new Error('ambiguous native field-status identity');
    error.code = 'invalid';
    throw error;
  }
  const issued = new Set(keys.map(([scope, person, field]) => keyOf(scope, person, field)));
  const before = new Map(previous.map((w) => [keyOf(w.scope, w.person_key, w.field), w.status]));
  const witnesses = [];
  for (const [key, entry] of [...current].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
    const newlyIssued = issued.has(key)
      && has(entry.status, 'revision') && jsonEqual(entry.status.revision, revision)
      && (!has(entry.status, 'review') || entry.status.review === null);
    const kept = before.has(key) && jsonEqual(before.get(key), entry.status);
    if (kept || newlyIssued) witnesses.push(entry);
  }
  // DELETE all rows of the record, INSERT the surviving ones: one document per record.
  await tx.put(WITNESSES, { id: recordId, record_id: recordId, witnesses, updated_at_ms: nowMs });
  return witnesses;
}
