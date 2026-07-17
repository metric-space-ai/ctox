// SYNC-13: browser-side collectionName → syncProfile registry.
//
// SYNC-32 added a per-collection `syncProfile` ("eager" | "demand-only" |
// "demand-chunks") wrapper sibling of `schema` in the collection definition,
// consumed NATIVELY (rxdb_peer.rs). The BROWSER still hardcodes its
// demand-only / demand-chunk classification lists in shared/sync.js. This
// registry closes that gap: at collection registration (rx-database.mjs
// `addCollections`, the same place that reads `conflictStrategy`) each
// collection's declared `syncProfile` is captured here, and shared/sync.js's
// demand classifiers consult it, FALLING BACK to their built-in static lists.
//
// Consequences:
//   - static collections (no declared profile) keep EXACTLY their built-in
//     classification — nothing regresses;
//   - a runtime-installed module declaring `syncProfile: 'demand-only'` or
//     `'demand-chunks'` is now correctly classified browser-side too.
//
// The registry is mirrored onto `globalThis` under a stable key so
// shared/sync.js (which lives OUTSIDE this bundle and deliberately avoids a
// static bundle import to prevent a duplicate module graph) can read it
// without importing the bundle, and so a single source of truth survives even
// if this module is evaluated more than once. Registration happens during
// shell schema registration, before `startWebRtcReplication`; a collection
// that somehow syncs before its profile is registered simply defaults to its
// built-in/eager classification and is reclassified once registered.

const REGISTRY_KEY = '__ctoxCollectionSyncProfiles';
// Only the two demand profiles override built-in classification. 'eager' is the
// default and is stored as "no override" (deleted), so the classifier's
// built-in path decides — identical to an undeclared collection.
const VALID_PROFILES = new Set(['demand-only', 'demand-chunks']);

function registryMap() {
  const existing = globalThis[REGISTRY_KEY];
  if (existing instanceof Map) return existing;
  const created = new Map();
  try {
    globalThis[REGISTRY_KEY] = created;
  } catch {
    // A frozen globalThis cannot host the mirror; callers still get the
    // module-scoped Map via this same instance.
  }
  return created;
}

export function registerCollectionSyncProfile(name, profile) {
  const key = String(name || '').trim();
  if (!key) return;
  if (!VALID_PROFILES.has(profile)) {
    // Undeclared / 'eager' / unknown → no override; the classifier keeps its
    // built-in default for this collection. Delete any stale prior value so a
    // re-registration is authoritative.
    registryMap().delete(key);
    return;
  }
  registryMap().set(key, profile);
}

export function getCollectionSyncProfile(name) {
  const key = String(name || '').trim();
  if (!key) return null;
  return registryMap().get(key) || null;
}

export function clearCollectionSyncProfiles() {
  registryMap().clear();
}
