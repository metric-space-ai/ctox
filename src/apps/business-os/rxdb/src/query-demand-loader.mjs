// V1.5 query demand loader.
//
// Sits between RxQuery.exec and the underlying storage. Asks the sidecar
// whether the requested (collection, fingerprint, window) is locally
// complete. If yes, returns local docs. If no AND a query-fetch transport is
// available, requests the missing window over WebRTC, writes the chunks into
// the existing `documents` store via the collection's storage layer, marks
// the sidecar window complete, and returns local docs.
//
// Cross-subscription dedup: identical (fingerprint, offset, limit) fetches
// in flight share one Promise. The dedup count is reported back via the
// status callback for diagnostics.

import { queryFingerprint } from './query-fingerprint.mjs';

export const DEFAULT_WINDOW_LIMIT = 200;

export function createQueryDemandLoader({
  storageCollection,
  sidecar,
  collectionName,
  schemaVersion,
  requestQueryFetch,
  requestCancel = null,
  multiTabBroker = null,
  status = null,
  clock = Date.now,
  // Origin stamp (object or provider fn) for every document this loader
  // writes into the primary store. Demand-fetched documents ARE master
  // state: without the stamp they counted as unsynced LOCAL writes, so the
  // push pipeline echoed them (and cache-eviction tombstones — i.e. DELETES)
  // back to the master, and the LWW gate let them veto later master pulls.
  replicationOrigin = null,
}) {
  if (!storageCollection) throw new TypeError('demand loader requires storageCollection');
  if (!sidecar) throw new TypeError('demand loader requires sidecar');
  if (!collectionName) throw new TypeError('demand loader requires collectionName');
  if (typeof requestQueryFetch !== 'function') {
    throw new TypeError('demand loader requires requestQueryFetch');
  }
  const resolveReplicationOrigin = () => (
    (typeof replicationOrigin === 'function' ? replicationOrigin() : replicationOrigin) || null
  );

  const inflightByFingerprint = new Map();

  return {
    async resolveQuery(query, { window } = {}) {
      const normalizedWindow = normalizeWindow(window, query);
      const fingerprintInput = {
        collection: collectionName,
        schemaVersion: schemaVersion ?? 0,
        selector: query?.selector ?? {},
        sort: normalizeSort(query?.sort),
        limit: query?.limit,
        skip: query?.skip,
        window: normalizedWindow,
      };
      const fingerprint = await queryFingerprint(fingerprintInput);
      const sidecarKey = [collectionName, fingerprint, normalizedWindow.offset, normalizedWindow.limit];

      const cached = await sidecar.getQueryWindow(sidecarKey);
      if (cached && cached.complete) {
        // V1.5 production hardening: authoritative-revision check. If the
        // caller supplies `requireRevision` (e.g. from a change-bulk that
        // touched a doc in the window), we re-verify with the server when
        // the cached revision is older. Otherwise the cache-hit path is fast.
        if (
          query?.requireRevision &&
          cached.authoritativeRevision !== query.requireRevision
        ) {
          // fall through to remote fetch
        } else {
          await touchSidecarAccess(sidecar, collectionName, cached.documentIds);
          return readLocalDocuments(storageCollection, query, normalizedWindow);
        }
      }

      const dedupKey = `${collectionName}|${fingerprint}|${normalizedWindow.offset}|${normalizedWindow.limit}`;
      if (inflightByFingerprint.has(dedupKey)) {
        bumpStatus(status, 'queryFetchDedupHitCount');
        return inflightByFingerprint.get(dedupKey);
      }

      bumpStatus(status, 'queryFetchInFlight', 1);
      v15Log('fetch:start', { collection: collectionName, fingerprint, offset: normalizedWindow.offset, limit: normalizedWindow.limit });
      const job = (async () => {
        const startedAt = clock();
        try {
          const result = await requestQueryFetch({
            requestId: `${dedupKey}|${startedAt}`,
            databaseName: storageCollection?.databaseName ?? null,
            collectionName,
            schemaVersion: schemaVersion ?? 0,
            queryFingerprint: fingerprint,
            query: {
              selector: query?.selector ?? {},
              sort: normalizeSort(query?.sort),
              limit: query?.limit,
              skip: query?.skip,
            },
            window: normalizedWindow,
          });
          await materializeChunks(storageCollection, result.documents || [], resolveReplicationOrigin());
          const documentIds = (result.documents || []).map(extractId).filter(Boolean);
          await sidecar.upsertQueryWindow({
            collection: collectionName,
            queryFingerprint: fingerprint,
            offset: normalizedWindow.offset,
            limit: normalizedWindow.limit,
            documentIds,
            complete: true,
            authoritativeRevision: result.authoritativeRevision ?? null,
          });
          await sidecar.touchDocuments(collectionName, documentIds, {
            estimatedBytes: estimateBytes(result.documents || []),
          });
          bumpStatus(status, 'queryFetchSuccessCount');
          if (status) status.lastQueryFetchMs = clock() - startedAt;
          v15Log('fetch:ok', { fingerprint, docs: documentIds.length, ms: clock() - startedAt });
          return readLocalDocuments(storageCollection, query, normalizedWindow);
        } catch (error) {
          if (isQueryCancelledError(error)) {
            bumpStatus(status, 'queryFetchCancelCount');
            v15Log('fetch:cancel', { fingerprint, error: String(error?.message ?? error) });
            return readLocalDocuments(storageCollection, query, normalizedWindow);
          }
          bumpStatus(status, 'queryFetchErrorCount');
          v15Log('fetch:error', { fingerprint, error: String(error?.message ?? error) });
          throw error;
        } finally {
          bumpStatus(status, 'queryFetchInFlight', -1);
          inflightByFingerprint.delete(dedupKey);
        }
      })();
      inflightByFingerprint.set(dedupKey, job);
      return job;
    },
    inflightSize() {
      return inflightByFingerprint.size;
    },

    // Wave 7: invalidation hook. When the replication layer reports that a
    // document in `collectionName` was changed remotely, call this with the
    // changed document ids — any cached query window that references those
    // ids is marked incomplete so the next exec triggers a remote refresh.
    async invalidateDocumentChange(changedDocumentIds = []) {
      if (!changedDocumentIds.length) return 0;
      const all = await sidecar.backend.scanQueryWindows();
      const ids = new Set(changedDocumentIds);
      let invalidated = 0;
      for (const window of all) {
        if (window.collection !== collectionName) continue;
        if (window.documentIds.some((id) => ids.has(id))) {
          await sidecar.invalidateQueryWindow([
            window.collection,
            window.queryFingerprint,
            window.offset,
            window.limit,
          ]);
          invalidated += 1;
        }
      }
      return invalidated;
    },

    // Wave 7 + production hardening: reconnect-cancel. Aborts all in-flight
    // fetches and removes any partially-materialized documents from the
    // primary store so the next fetch starts from a clean slate (no orphans).
    async abortAllInFlight(reason = 'reconnect') {
      const cancelled = [];
      for (const [dedupKey, job] of inflightByFingerprint.entries()) {
        const [, fingerprint] = dedupKey.split('|');
        cancelled.push({ dedupKey, fingerprint });
        try {
          job.catch?.(() => {});
        } catch {}
        if (typeof requestCancel === 'function') {
          try {
            await requestCancel({ requestId: dedupKey, fingerprint, reason });
          } catch {
            // best-effort cancel
          }
        }
      }
      inflightByFingerprint.clear();

      // Orphan cleanup: for every fingerprint that had an in-flight fetch
      // but no complete window in the sidecar, drop the partial document
      // IDs from the primary store. This prevents the cache from accreting
      // half-materialized data across reconnects.
      try {
        const allWindows = await sidecar.backend.scanQueryWindows();
        for (const { fingerprint } of cancelled) {
          // Any window with this fingerprint that is NOT complete — its
          // referenced IDs are partial, untrusted.
          const partial = allWindows.filter(
            (w) => w.queryFingerprint === fingerprint && !w.complete,
          );
          for (const window of partial) {
            const ids = window.documentIds || [];
            if (ids.length && typeof storageCollection.bulkWrite === 'function') {
              // Mark each as deleted in the primary store. We can't reach
              // into the underlying SQLite DELETE from here, but soft-delete
              // via _deleted=true is enough for the cache layer.
              const tombstones = ids.map((id) => ({ id, _deleted: true }));
              // Cache bookkeeping, NOT a user delete: stamp the replication
              // origin so the push pipeline never replays these tombstones
              // to the master as real deletions.
              try { await storageCollection.bulkWrite(tombstones, { replicationOrigin: resolveReplicationOrigin() }); } catch {}
            }
            await sidecar.backend.deleteQueryWindow([
              window.collection,
              window.queryFingerprint,
              window.offset,
              window.limit,
            ]);
          }
        }
      } catch {
        // best-effort cleanup; never throw upstream from an abort path
      }
    },

    // Wave 7: multi-tab dedup. If a `multiTabBroker` is provided, it is
    // consulted before kicking off a remote fetch; followers wait for the
    // leader's materialization signal instead of fetching themselves.
    async leaderClaim(windowKey) {
      if (!multiTabBroker?.claim) return true;
      return multiTabBroker.claim(windowKey);
    },
    async leaderRelease(windowKey) {
      if (!multiTabBroker?.release) return;
      await multiTabBroker.release(windowKey);
    },
  };
}

function normalizeWindow(window, query) {
  if (window && typeof window === 'object') {
    return {
      offset: Math.max(0, Math.floor(Number(window.offset) || 0)),
      limit: Math.max(1, Math.floor(Number(window.limit) || DEFAULT_WINDOW_LIMIT)),
    };
  }
  return {
    offset: Math.max(0, Math.floor(Number(query?.skip) || 0)),
    limit: Math.max(1, Math.floor(Number(query?.limit) || DEFAULT_WINDOW_LIMIT)),
  };
}

function normalizeSort(sort) {
  if (!Array.isArray(sort)) return [];
  return sort.map((entry) => {
    if (!entry || typeof entry !== 'object') return entry;
    const keys = Object.keys(entry);
    if (keys.length !== 1) return entry;
    const key = keys[0];
    const direction = entry[key];
    return { [key]: direction === -1 || direction === 'desc' || direction === 'DESC' ? 'desc' : 'asc' };
  });
}

async function readLocalDocuments(storageCollection, query, window) {
  if (typeof storageCollection.queryDocuments === 'function') {
    return storageCollection.queryDocuments(
      { ...query, skip: window.offset, limit: window.limit },
      {
        matchesSelector: defaultMatcher,
        sortDocuments: defaultSorter,
      },
    );
  }
  const docs = await storageCollection.allDocuments();
  return applyQueryToDocs(docs, query, window);
}

async function materializeChunks(storageCollection, documents, replicationOrigin = null) {
  if (!documents.length) return;
  await storageCollection.bulkWrite(documents, { replicationOrigin });
}

async function touchSidecarAccess(sidecar, collectionName, documentIds) {
  if (!documentIds?.length) return;
  await sidecar.touchDocuments(collectionName, documentIds);
}

function extractId(doc) {
  if (!doc || typeof doc !== 'object') return null;
  return doc.id || doc._id || null;
}

function estimateBytes(documents) {
  try {
    return JSON.stringify(documents).length;
  } catch {
    return documents.length * 256;
  }
}

function bumpStatus(status, field, delta = 1) {
  if (!status) return;
  if (typeof status[field] !== 'number') status[field] = 0;
  status[field] += delta;
}

function isQueryCancelledError(error) {
  return error?.code === 'QUERY_CANCELLED'
    || String(error?.message || '').includes('QUERY_CANCELLED');
}

let v15LogSink = null;
export function setV15LogSink(fn) { v15LogSink = typeof fn === 'function' ? fn : null; }
function v15Log(event, fields) {
  if (v15LogSink) {
    try { v15LogSink(event, fields); } catch {}
    return;
  }
  if (globalThis?.console?.debug) {
    globalThis.console.debug('[V1.5]', event, fields);
  }
}

function defaultMatcher(doc, selector = {}) {
  for (const [key, expected] of Object.entries(selector)) {
    if (key.startsWith('$')) return true; // delegate complex operators to storage layer
    const actual = doc?.[key];
    if (expected && typeof expected === 'object' && !Array.isArray(expected)) {
      if ('$eq' in expected && actual !== expected.$eq) return false;
      if ('$ne' in expected && actual === expected.$ne) return false;
      if ('$in' in expected && !expected.$in.includes(actual)) return false;
      if ('$gte' in expected && !(actual >= expected.$gte)) return false;
      if ('$lte' in expected && !(actual <= expected.$lte)) return false;
      continue;
    }
    if (actual !== expected) return false;
  }
  return true;
}

function defaultSorter(docs, sort = []) {
  if (!sort?.length) return docs;
  return docs.slice().sort((a, b) => {
    for (const entry of sort) {
      const [key, direction] = Object.entries(entry)[0] || [];
      const factor = direction === 'desc' ? -1 : 1;
      const av = a?.[key];
      const bv = b?.[key];
      if (av < bv) return -1 * factor;
      if (av > bv) return 1 * factor;
    }
    return 0;
  });
}

function applyQueryToDocs(docs, query, window) {
  let filtered = (docs || []).filter((doc) => defaultMatcher(doc, query?.selector || {}));
  filtered = defaultSorter(filtered, normalizeSort(query?.sort));
  if (window.offset > 0) filtered = filtered.slice(window.offset);
  if (Number.isFinite(window.limit)) filtered = filtered.slice(0, window.limit);
  return filtered;
}
