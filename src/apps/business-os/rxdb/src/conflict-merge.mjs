// Field-merge conflict strategy (opt-in per collection).
//
// Default CTOX Sync Engine semantics are whole-document LWW with the origin-aware gate
// in storage-indexeddb.mjs (§8.1 invariants): an unsynced LOCAL write wins
// whole-doc against an incoming master row, or is replaced whole-doc once the
// master row is accepted — either way one side's FIELD changes are lost when
// two people edit the same record concurrently.
//
// Collections that declare `conflictStrategy: 'field-merge'` (a sibling of
// `schema` in the collection definition — deliberately OUTSIDE the schema
// object so schema hashes do not change) instead merge at top-level-field
// granularity using a three-way merge:
//
//   - base   = the last master-confirmed state before the local edit
//              (tracked by the storage layer as `record.base`)
//   - local  = the unsynced local document
//   - master = the incoming replicated document
//
// Per business field: if only one side changed it relative to base, that side
// wins; if BOTH changed it, the LOCAL value wins (consistent with the
// existing unsynced-local-write-wins rule, but scoped to the field — the
// merged doc stays pushable and round-trips through the master). Deletions
// stay whole-doc: a master tombstone is accepted outright, an unsynced local
// tombstone survives until it pushes.
//
// The merge is browser-side only. The native master needs no counterpart:
// the fork (browser) resolves conflicts in the RxDB replication model, and a
// merged doc is pushed like any local write.

const SYSTEM_FIELD_PREFIX = '_';

export function deepEqualJson(a, b) {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== typeof b) return false;
  if (typeof a !== 'object') return false;
  const aIsArray = Array.isArray(a);
  if (aIsArray !== Array.isArray(b)) return false;
  if (aIsArray) {
    if (a.length !== b.length) return false;
    for (let index = 0; index < a.length; index += 1) {
      if (!deepEqualJson(a[index], b[index])) return false;
    }
    return true;
  }
  const aKeys = Object.keys(a);
  const bKeys = Object.keys(b);
  if (aKeys.length !== bKeys.length) return false;
  for (const key of aKeys) {
    if (!Object.prototype.hasOwnProperty.call(b, key)) return false;
    if (!deepEqualJson(a[key], b[key])) return false;
  }
  return true;
}

function businessFieldKeys(...docs) {
  const keys = new Set();
  for (const doc of docs) {
    if (!doc || typeof doc !== 'object') continue;
    for (const key of Object.keys(doc)) {
      if (!key.startsWith(SYSTEM_FIELD_PREFIX)) keys.add(key);
    }
  }
  return keys;
}

// Three-way merge of base/local/master at top-level business-field
// granularity. Returns `{ merged, identicalToMaster }`:
//   - `identicalToMaster: true` means no local-only change survived — the
//     caller should store the master row unchanged (as a replicated write, so
//     the row leaves the push set and the base is cleared).
//   - otherwise `merged` carries surviving local changes and MUST be stored
//     as a LOCAL (pushable) write with the incoming master doc as new base.
export function threeWayMergeDocuments(base, local, master, { primaryPath = 'id' } = {}) {
  const safeBase = base && typeof base === 'object' ? base : {};
  const safeLocal = local && typeof local === 'object' ? local : {};
  const safeMaster = master && typeof master === 'object' ? master : {};

  // Deletions stay whole-doc (mirrors the LWW gate's semantics).
  if (safeMaster._deleted) {
    return { merged: safeMaster, identicalToMaster: true };
  }
  if (safeLocal._deleted) {
    return { merged: safeLocal, identicalToMaster: false };
  }

  // System fields (_meta, _rev, _attachments, …) always come from the master
  // row; the storage layer re-stamps them for the chosen write kind anyway.
  const merged = {};
  for (const key of Object.keys(safeMaster)) {
    if (key.startsWith(SYSTEM_FIELD_PREFIX)) merged[key] = safeMaster[key];
  }
  merged[primaryPath] = safeMaster[primaryPath] ?? safeLocal[primaryPath];

  let localOnlyChange = false;
  const unsafeStructuredConflictFields = [];
  for (const key of businessFieldKeys(safeBase, safeLocal, safeMaster)) {
    if (key === primaryPath) continue;
    const baseValue = safeBase[key];
    const localValue = safeLocal[key];
    const masterValue = safeMaster[key];
    const localChanged = !deepEqualJson(localValue, baseValue);
    const masterChanged = !deepEqualJson(masterValue, baseValue);
    if (
      localChanged
      && masterChanged
      && !deepEqualJson(localValue, masterValue)
      && (isStructuredValue(localValue) || isStructuredValue(masterValue))
    ) {
      unsafeStructuredConflictFields.push(key);
    }
    const winner = localChanged ? localValue : masterValue;
    if (localChanged && !deepEqualJson(localValue, masterValue)) {
      localOnlyChange = true;
    }
    if (winner !== undefined) {
      merged[key] = winner;
    }
  }

  return {
    merged,
    identicalToMaster: !localOnlyChange,
    requiresManualResolution: unsafeStructuredConflictFields.length > 0,
    conflictFields: unsafeStructuredConflictFields,
  };
}

function isStructuredValue(value) {
  return value !== null && typeof value === 'object';
}

export function normalizeConflictStrategy(value) {
  return value === 'field-merge' ? 'field-merge' : 'lww';
}

// SYNC-41: delete strategy is an INDEPENDENT sibling of `conflictStrategy`
// (a collection may want field-merge updates AND final deletes), declared like
// `schema`/`conflictStrategy` outside the schema object so schema hashes are
// unaffected. `deleteStrategy: 'final'` makes a tombstone ALWAYS win over a
// concurrent non-tombstone update regardless of HLC/lwt, on every path that
// decides a delete-vs-update: the pull gate, the push-conflict path and the
// field-merge path. Once deleted, a concurrent update can never resurrect the
// row; a delete and an update racing → delete wins deterministically on all
// peers. `default` keeps exactly today's whole-doc LWW delete semantics (a
// master tombstone is accepted by lwt/HLC ordering, but a higher-HLC local
// update can win over — and resurrect — a tombstone).
export function normalizeDeleteStrategy(value) {
  return value === 'final' ? 'final' : 'default';
}
