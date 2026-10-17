// Origin: CTOX
// License: AGPL-3.0-only
//
// Native-owned document fields, browser mirror.
//
// The native masterWrite guard (src/core/business_os/outbound_field_review.rs,
// `peer_lead_write_allowed`) refuses a WHOLE batch when one incoming row's
// stamped field statuses differ from the stored master: field statuses with a
// non-null `revision` or `review` are writeback receipts / reviews only the
// native side may issue. It also refuses a researched lead that a peer sets
// back to `failed`.
//
// The browser pushes whole documents. A local document whose copy of those
// native-owned values went stale (the native side wrote them after the
// browser's last master state) was therefore refused forever, even when the
// user only changed an ordinary field such as the campaign. Rebasing replaces
// exactly the native-owned values with the master's and leaves every other
// field of the local document — the user's change — untouched.

// ref: src/core/business_os/outbound_field_review.rs `COLLECTION`
export const NATIVE_OWNED_FIELD_COLLECTIONS = new Set(['outbound_lead_generation_leads']);

const RESEARCHED_STATUSES = new Set(['needs_review', 'completed']);

export function collectionHasNativeOwnedFields(collectionName) {
  return NATIVE_OWNED_FIELD_COLLECTIONS.has(String(collectionName || ''));
}

export function isNativeOwnedRejectionMessage(message) {
  return /cannot change native-owned/i.test(String(message || ''));
}

// ref: outbound_field_review.rs `stamped`
function stamped(status) {
  return Boolean(status && typeof status === 'object' && !Array.isArray(status)
    && ['revision', 'review'].some((key) => status[key] !== undefined && status[key] !== null));
}

function plainObject(value) {
  return value && typeof value === 'object' && !Array.isArray(value) ? value : null;
}

function clone(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

// Peer-writable (unstamped) local entries, overlaid with the master's stamped
// entries. A stamped local entry the master does not carry falls back to the
// master's unstamped value or disappears: only the master may stamp.
function rebaseStatusMap(local, master) {
  const localMap = plainObject(local);
  const masterMap = plainObject(master);
  if (!localMap && !masterMap) return local;
  const result = {};
  for (const [field, status] of Object.entries(localMap || {})) {
    if (!stamped(status)) {
      result[field] = status;
    } else if (masterMap && Object.prototype.hasOwnProperty.call(masterMap, field) && !stamped(masterMap[field])) {
      result[field] = clone(masterMap[field]);
    }
  }
  for (const [field, status] of Object.entries(masterMap || {})) {
    if (stamped(status)) result[field] = clone(status);
  }
  if (!localMap && !Object.keys(result).length) return local;
  return result;
}

function contactsByPersonKey(contacts) {
  const byKey = new Map();
  for (const contact of Array.isArray(contacts) ? contacts : []) {
    const key = typeof contact?.person_key === 'string' ? contact.person_key : '';
    if (!key) continue;
    byKey.set(key, byKey.has(key) ? null : contact);
  }
  return byKey;
}

/// Returns a copy of `local` whose native-owned values equal `master`'s. Every
/// other field keeps the local value. Collections without native-owned fields
/// and a missing master state return `local` unchanged.
export function rebaseNativeOwnedFields(collectionName, local, master) {
  if (!collectionHasNativeOwnedFields(collectionName)) return local;
  if (!plainObject(local) || !plainObject(master) || local._deleted) return local;
  const next = { ...local };
  const rebasedLead = rebaseStatusMap(local.field_status, master.field_status);
  if (rebasedLead !== undefined) next.field_status = rebasedLead;

  const localPeople = plainObject(local.person_field_status);
  const masterPeople = plainObject(master.person_field_status);
  if (localPeople || masterPeople) {
    const people = {};
    for (const [person, statuses] of Object.entries(localPeople || {})) {
      people[person] = rebaseStatusMap(statuses, masterPeople?.[person]);
    }
    for (const [person, statuses] of Object.entries(masterPeople || {})) {
      if (Object.prototype.hasOwnProperty.call(people, person)) continue;
      if (Object.values(plainObject(statuses) || {}).some(stamped)) {
        people[person] = rebaseStatusMap(undefined, statuses);
      }
    }
    next.person_field_status = people;
  }

  if (Array.isArray(local.contacts)) {
    const masterContacts = contactsByPersonKey(master.contacts);
    next.contacts = local.contacts.map((contact) => {
      if (!plainObject(contact)) return contact;
      const key = typeof contact.person_key === 'string' ? contact.person_key : '';
      const masterContact = key ? masterContacts.get(key) : null;
      const statuses = rebaseStatusMap(contact.field_status, masterContact?.field_status);
      if (statuses === contact.field_status) return contact;
      return { ...contact, field_status: statuses };
    });
  }

  // ref: outbound_field_review.rs `peer_fails_researched_lead`
  if (local.research_status === 'failed' && RESEARCHED_STATUSES.has(master.research_status)) {
    next.research_status = master.research_status;
  }
  return next;
}
