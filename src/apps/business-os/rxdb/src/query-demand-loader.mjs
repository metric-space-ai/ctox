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
export const DEFAULT_QUERY_WINDOW_REVALIDATE_MS = 30_000;
const CONTROL_PLANE_QUERY_REVALIDATE_MS = 1000;
const TRACKED_CONTROL_PLANE_QUERY_REVALIDATE_MS = 250;
const ACTIVE_COMMAND_STORAGE_KEY = 'ctox.businessOs.activeCommandIds.v1';
const EMPTY_QUERY_WINDOW_REVALIDATE_MS = 5000;
const MUTABLE_QUERY_MEMBERSHIP_REVALIDATE_MS = 5000;

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
  queryWindowRevalidateMs = DEFAULT_QUERY_WINDOW_REVALIDATE_MS,
  // Opaque identity of the bridge/connection generation. Strict
  // requireRevision reads must never cross this boundary.
  queryGeneration = null,
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
  const boundedQueryWindowRevalidateMs = Math.max(
    250,
    Number(queryWindowRevalidateMs) || DEFAULT_QUERY_WINDOW_REVALIDATE_MS,
  );

  const inflightByFingerprint = new Map();
  const coordinatedByFingerprint = new Map();
  let nextRequestSequence = 0;
  // Install an invocation-level promise before the first fingerprint/sidecar
  // await. Otherwise a very fast fetch can complete while sibling calls are
  // still hashing, causing a concurrent caller to look like a later cache hit
  // instead of sharing the original operation.
  const resolvingByInput = new Map();
  const consumerSignals = new WeakMap();
  let nextConsumerSignalSequence = 0;

  return {
    async resolveQuery(query, { window, signal } = {}) {
      const normalizedWindow = normalizeWindow(window, query);
      const strictRequireRevision = Boolean(query?.requireRevision);
      const generation = strictRequireRevision ? String(queryGeneration?.() || '') : '';
      if (strictRequireRevision && !generation) {
        throw new Error('QUERY_GENERATION_REQUIRED: strict demand read has no bridge generation');
      }
      const consumerSignal = signal && typeof signal.addEventListener === 'function' ? signal : null;
      let signalToken = '';
      if (consumerSignal) {
        signalToken = consumerSignals.get(consumerSignal)
          || `signal-${nextConsumerSignalSequence += 1}`;
        consumerSignals.set(consumerSignal, signalToken);
      }
      const fingerprintInput = {
        collection: collectionName,
        schemaVersion: schemaVersion ?? 0,
        selector: query?.selector ?? {},
        sort: normalizeSort(query?.sort),
        limit: query?.limit,
        skip: query?.skip,
        window: normalizedWindow,
      };
      const inputKey = JSON.stringify({
        ...fingerprintInput,
        requireRevision: query?.requireRevision ?? null,
        requireGeneration: generation || null,
        signalToken: signalToken || null,
      });
      const existingInvocation = resolvingByInput.get(inputKey);
      if (existingInvocation) {
        bumpStatus(status, 'queryFetchDedupHitCount');
        return existingInvocation.job;
      }
      const requestId = `${collectionName}|query|${clock()}|${nextRequestSequence += 1}`;
      const invocationEntry = {
        job: null,
        requestId,
        fingerprint: null,
        cancelledReason: null,
        rejectCancellation: null,
        consumerCancelled: false,
        detachConsumerSignal: null,
      };
      const cancellationPromise = new Promise((_, reject) => {
        invocationEntry.rejectCancellation = reject;
      });
      cancellationPromise.catch(() => {});
      if (consumerSignal) {
        const abortConsumer = () => {
          if (invocationEntry.cancelledReason) return;
          invocationEntry.consumerCancelled = true;
          invocationEntry.cancelledReason = 'consumer-abort';
          invocationEntry.rejectCancellation?.(createQueryCancelledError('consumer-abort'));
          Promise.resolve().then(() => requestCancel?.({
            requestId: invocationEntry.requestId,
            fingerprint: invocationEntry.fingerprint,
            reason: 'consumer-abort',
          })).catch(() => {});
        };
        if (consumerSignal.aborted) abortConsumer();
        else consumerSignal.addEventListener('abort', abortConsumer, { once: true });
        invocationEntry.detachConsumerSignal = () => {
          consumerSignal.removeEventListener('abort', abortConsumer);
          invocationEntry.detachConsumerSignal = null;
        };
      }
      const assertFresh = () => {
        throwIfQueryCancelled(invocationEntry);
        if (strictRequireRevision && queryGeneration?.() !== generation) {
          throw createQueryGenerationChangedError();
        }
      };
      const invocationJob = (async () => {
      const fingerprint = await queryFingerprint(fingerprintInput);
      invocationEntry.fingerprint = fingerprint;
      assertFresh();
      const sidecarKey = [collectionName, fingerprint, normalizedWindow.offset, normalizedWindow.limit];

      const cached = await sidecar.getQueryWindow(sidecarKey);
      assertFresh();
      const cachedDocumentsAvailable = await queryWindowDocumentsAvailable(
        storageCollection,
        cached?.documentIds,
      );
      assertFresh();
      if (cached && (cached.complete || cached.everCompleted) && !cachedDocumentsAvailable) {
        await sidecar.invalidateQueryWindow(sidecarKey);
        cached.complete = false;
        bumpStatus(status, 'queryFetchEvictedWindowMissCount');
      }
      const controlPlaneRevalidateMs = controlPlaneQueryRevalidateMs(collectionName, query);
      const controlPlaneWindowStale = isControlPlaneStatusCollection(collectionName)
        && cached
        && clock() - Number(cached.updatedAt || cached.createdAt || 0) >= controlPlaneRevalidateMs;
      // An empty response can be authoritative at fetch time and still become
      // stale without a local document change to invalidate it. This happens
      // during projection/startup races: the browser queries before the native
      // peer has materialized a table, then otherwise caches "empty" forever.
      const emptyWindowStale = cached
        && (!Array.isArray(cached.documentIds) || cached.documentIds.length === 0)
        && clock() - Number(cached.updatedAt || cached.createdAt || 0) >= EMPTY_QUERY_WINDOW_REVALIDATE_MS;
      // A change event can invalidate only windows that already reference the
      // changed document. Newly projected knowledge tables are absent from an
      // older non-empty membership list, so no local event can identify that
      // window as stale. Revalidate this small table-directory query with SWR
      // semantics; table chunks themselves remain demand-loaded by ID.
      const mutableMembershipWindowStale = isMutableMembershipCollection(collectionName)
        && cached
        && Array.isArray(cached.documentIds)
        && cached.documentIds.length > 0
        && clock() - Number(cached.updatedAt || cached.createdAt || 0)
          >= MUTABLE_QUERY_MEMBERSHIP_REVALIDATE_MS;
      const queryWindowStale = cached
        && clock() - Number(cached.updatedAt || cached.createdAt || 0)
          >= boundedQueryWindowRevalidateMs;
      if (
        cached
        && cached.complete
        && cachedDocumentsAvailable
        && !emptyWindowStale
        && !mutableMembershipWindowStale
        && !queryWindowStale
      ) {
        if (strictRequireRevision) {
          // Same token plus exact bridge/connection/database generation may
          // reuse the authority result. Any new hydration token or replacement
          // generation must fetch again.
          if (
            cached.satisfiedRevision === query.requireRevision
            && cached.satisfiedGeneration === generation
            && !controlPlaneWindowStale
          ) {
            await touchSidecarAccess(sidecar, collectionName, cached.documentIds);
            return readLocalDocuments(
              storageCollection,
              query,
              normalizedWindow,
              cached.documentIds,
            );
          }
        } else if (!controlPlaneWindowStale) {
          await touchSidecarAccess(sidecar, collectionName, cached.documentIds);
          return readLocalDocuments(
            storageCollection,
            query,
            normalizedWindow,
            cached.documentIds,
          );
        }
      }

      const dedupKey = strictRequireRevision
        ? `${collectionName}|${fingerprint}|${normalizedWindow.offset}|${normalizedWindow.limit}|strict|${query.requireRevision}|${generation}`
        : `${collectionName}|${fingerprint}|${normalizedWindow.offset}|${normalizedWindow.limit}`;
      const startFetchJob = () => {
        if (inflightByFingerprint.has(dedupKey)) {
          bumpStatus(status, 'queryFetchDedupHitCount');
          return inflightByFingerprint.get(dedupKey).job;
        }
        bumpStatus(status, 'queryFetchInFlight', 1);
        v15Log('fetch:start', { collection: collectionName, fingerprint, offset: normalizedWindow.offset, limit: normalizedWindow.limit });
        throwIfQueryCancelled(invocationEntry);
        const job = (async () => {
        const startedAt = clock();
        try {
          assertFresh();
          const result = await Promise.race([
            requestQueryFetch({
              requestId,
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
            }),
            cancellationPromise,
          ]);
          assertFresh();
          await materializeChunks(storageCollection, result.documents || [], resolveReplicationOrigin());
          assertFresh();
          const documentIds = (result.documents || []).map(extractId).filter(Boolean);
          await sidecar.upsertQueryWindow({
            collection: collectionName,
            queryFingerprint: fingerprint,
            offset: normalizedWindow.offset,
            limit: normalizedWindow.limit,
            documentIds,
            complete: true,
            authoritativeRevision: result.authoritativeRevision ?? null,
            satisfiedRevision: query?.requireRevision ?? null,
            satisfiedGeneration: strictRequireRevision ? generation : null,
            queryShape: {
              selector: query?.selector ?? {},
              sort: normalizeSort(query?.sort),
            },
          });
          assertFresh();
          await sidecar.touchDocuments(collectionName, documentIds, {
            estimatedBytes: estimateBytesPerDocument(result.documents || []),
          });
          assertFresh();
          bumpStatus(status, 'queryFetchSuccessCount');
          if (status) status.lastQueryFetchMs = clock() - startedAt;
          v15Log('fetch:ok', { fingerprint, docs: documentIds.length, ms: clock() - startedAt });
          return authoritativeFetchedDocuments(result.documents || [], documentIds);
        } catch (error) {
          if (isQueryCancelledError(error)) {
            bumpStatus(status, 'queryFetchCancelCount');
            v15Log('fetch:cancel', { fingerprint, error: String(error?.message ?? error) });
            // A strict authority token has no local fallback. An explicit
            // consumer abort must not silently become local data either.
            if (strictRequireRevision || invocationEntry.consumerCancelled) throw error;
            return readLocalDocuments(
              storageCollection,
              query,
              normalizedWindow,
              cached?.documentIds,
            );
          }
          bumpStatus(status, 'queryFetchErrorCount');
          v15Log('fetch:error', { fingerprint, error: String(error?.message ?? error) });
          throw error;
        } finally {
          bumpStatus(status, 'queryFetchInFlight', -1);
          inflightByFingerprint.delete(dedupKey);
        }
        })();
        inflightByFingerprint.set(dedupKey, { job, requestId });
        return job;
      };

      const runCoordinatedFetchJob = async () => {
        if (!multiTabBroker?.claim) return startFetchJob();
        if (multiTabBroker.closed) {
          if (strictRequireRevision || invocationEntry.consumerCancelled) {
            throw createQueryCancelledError('multi-tab-broker-closed');
          }
          return readLocalDocuments(
            storageCollection,
            query,
            normalizedWindow,
            cached?.documentIds,
          );
        }
        assertFresh();
        const leader = await multiTabBroker.claim(dedupKey);
        assertFresh();
        if (leader) {
          try {
            return await startFetchJob();
          } finally {
            await multiTabBroker.release?.(dedupKey, { materialized: true });
          }
        }
        await multiTabBroker.waitForRemote?.(dedupKey, 5_000);
        assertFresh();
        if (multiTabBroker.closed) {
          if (strictRequireRevision || invocationEntry.consumerCancelled) {
            throw createQueryCancelledError('multi-tab-broker-closed');
          }
          return readLocalDocuments(
            storageCollection,
            query,
            normalizedWindow,
            cached?.documentIds,
          );
        }
        const materialized = await sidecar.getQueryWindow(sidecarKey);
        assertFresh();
        if (
          materialized?.complete
          && await queryWindowDocumentsAvailable(storageCollection, materialized.documentIds)
          && (
            !strictRequireRevision
            || (
              materialized.satisfiedRevision === query.requireRevision
              && materialized.satisfiedGeneration === generation
            )
          )
        ) {
          bumpStatus(status, 'queryFetchDedupHitCount');
          return readLocalDocuments(
            storageCollection,
            query,
            normalizedWindow,
            materialized.documentIds,
          );
        }
        // The owner may have crashed. Bounded wait plus TTL-aware re-claim
        // lets this tab take over without leaving the query hung forever.
        const takeover = await multiTabBroker.claim(dedupKey);
        assertFresh();
        if (!takeover) {
          if (multiTabBroker.closed) {
            if (strictRequireRevision || invocationEntry.consumerCancelled) {
              throw createQueryCancelledError('multi-tab-broker-closed');
            }
            return readLocalDocuments(
              storageCollection,
              query,
              normalizedWindow,
              cached?.documentIds,
            );
          }
          // A dead/replaced collection state can leave a 30 s broker claim
          // behind. We already waited the full bounded follower window; a
          // duplicate idempotent query fetch is safer than freezing every
          // module until that stale claim expires. The materialization write
          // remains conflict-safe and the live owner, if any, can complete in
          // parallel.
          return startFetchJob();
        }
        try {
          return await startFetchJob();
        } finally {
          await multiTabBroker.release?.(dedupKey, { materialized: true, takeover: true });
        }
      };
      const coordinatedFetchJob = () => {
        const current = coordinatedByFingerprint.get(dedupKey);
        if (current) {
          bumpStatus(status, 'queryFetchDedupHitCount');
          return current;
        }
        // Install the local coordination promise synchronously, before the
        // first await inside the broker election. Otherwise two identical
        // same-tab queries can both enter claim(); the second then mistakes
        // this tab's own live claim for a remote owner and times out because a
        // BroadcastChannel never echoes completion to its sender object.
        const job = Promise.resolve()
          .then(runCoordinatedFetchJob)
          .finally(() => {
            if (coordinatedByFingerprint.get(dedupKey) === job) {
              coordinatedByFingerprint.delete(dedupKey);
            }
          });
        coordinatedByFingerprint.set(dedupKey, job);
        return job;
      };

      // Stale-while-revalidate: a window that was EVER complete has its
      // member documents in the primary store, and replication keeps those
      // documents fresh — an invalidation only means the window MEMBERSHIP
      // may have changed. Serve local results immediately and revalidate in
      // the background; the materialised refresh emits a storage change
      // event, so reactive queries re-render on arrival. This turns repeat
      // module loads from a WebRTC round-trip into an IndexedDB read.
      // An explicit requireRevision keeps strict await semantics.
      if (
        cached?.everCompleted
        && cachedDocumentsAvailable
        && !emptyWindowStale
        && !query?.requireRevision
      ) {
        if (controlPlaneWindowStale) {
          // Commands and queue tasks are demand-only to avoid replaying the
          // complete historical ledger. Their records are mutable lifecycle
          // projections, though, so a completed query window cannot remain a
          // permanent cache hit. Await the bounded, deduplicated ID/window
          // refresh once its short freshness budget expires.
          return coordinatedFetchJob();
        }
        coordinatedFetchJob().catch(() => {
          // Surfaced via queryFetchErrorCount; the next exec retries.
        });
        bumpStatus(status, 'queryFetchStaleServedCount');
        v15Log('fetch:stale-served', { collection: collectionName, fingerprint, offset: normalizedWindow.offset, limit: normalizedWindow.limit });
        await touchSidecarAccess(sidecar, collectionName, cached.documentIds || []);
        return readLocalDocuments(
          storageCollection,
          query,
          normalizedWindow,
          cached.documentIds || [],
        );
      }

      return coordinatedFetchJob();
      })();
      invocationEntry.job = invocationJob;
      resolvingByInput.set(inputKey, invocationEntry);
      try {
        return await invocationJob;
      } finally {
        invocationEntry.detachConsumerSignal?.();
        if (resolvingByInput.get(inputKey)?.job === invocationJob) resolvingByInput.delete(inputKey);
        invocationEntry.rejectCancellation = null;
        invocationEntry.detachConsumerSignal = null;
      }
    },
    inflightSize() {
      return Math.max(inflightByFingerprint.size, coordinatedByFingerprint.size, resolvingByInput.size);
    },

    // Wave 7: invalidation hook. When the replication layer reports that a
    // document in `collectionName` was changed remotely, call this with the
    // changed document ids — any cached query window that references those
    // ids is marked incomplete so the next exec triggers a remote refresh.
    async invalidateDocumentChange(changedDocumentIds = []) {
      if (!changedDocumentIds.length) return 0;
      if (typeof sidecar.invalidateQueryWindowsForDocuments === 'function') {
        return sidecar.invalidateQueryWindowsForDocuments(collectionName, changedDocumentIds);
      }
      return invalidateByScanningQueryWindows(sidecar, collectionName, changedDocumentIds);
    },

    async invalidateDocuments(changedDocuments = []) {
      if (!changedDocuments.length) return 0;
      if (typeof sidecar.invalidateQueryWindowsForChanges === 'function') {
        return sidecar.invalidateQueryWindowsForChanges(
          collectionName,
          changedDocuments,
          storageCollection?.primaryPath || 'id',
        );
      }
      return this.invalidateDocumentChange(changedDocuments.map(extractId).filter(Boolean));
    },

    // Wave 7 + production hardening: reconnect-cancel. Aborts all in-flight
    // fetches and removes any partially-materialized documents from the
    // primary store so the next fetch starts from a clean slate (no orphans).
    async abortAllInFlight(reason = 'reconnect') {
      const cancelled = [];
      const cancellationTargets = new Map();
      for (const entry of resolvingByInput.values()) {
        cancellationTargets.set(entry.requestId, entry.fingerprint);
        if (!entry.cancelledReason) {
          entry.cancelledReason = reason;
          const error = createQueryCancelledError(reason);
          entry.rejectCancellation?.(error);
        }
        try {
          entry.job?.catch?.(() => {});
        } catch {}
      }
      for (const [dedupKey, entry] of inflightByFingerprint.entries()) {
        const { job, requestId } = entry;
        const [, fingerprint] = dedupKey.split('|');
        cancellationTargets.set(requestId, fingerprint);
        try {
          job.catch?.(() => {});
        } catch {}
      }
      for (const [requestId, fingerprint] of cancellationTargets.entries()) {
        if (fingerprint) cancelled.push({ requestId, fingerprint });
        if (typeof requestCancel === 'function') {
          try {
            await requestCancel({ requestId, fingerprint, reason });
          } catch {
            // best-effort cancel
          }
        }
      }
      inflightByFingerprint.clear();
      coordinatedByFingerprint.clear();
      resolvingByInput.clear();

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
            // Ever-complete windows hold replicated (validated) documents;
            // an aborted background revalidation must not tombstone them.
            // Only never-completed windows can reference partial orphans.
            (w) => w.queryFingerprint === fingerprint && !w.complete && !w.everCompleted,
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

function isControlPlaneStatusCollection(collectionName) {
  return collectionName === 'business_commands' || collectionName === 'ctox_queue_tasks';
}

function controlPlaneQueryRevalidateMs(collectionName, query) {
  if (!isControlPlaneStatusCollection(collectionName)) return CONTROL_PLANE_QUERY_REVALIDATE_MS;
  const selector = query?.selector || {};
  const trackedId = exactSelectorValue(selector.id)
    || exactSelectorValue(selector.command_id)
    || exactSelectorValue(selector.commandId);
  if (!trackedId) return CONTROL_PLANE_QUERY_REVALIDATE_MS;
  try {
    const activeIds = JSON.parse(
      globalThis.localStorage?.getItem?.(ACTIVE_COMMAND_STORAGE_KEY) || '[]',
    );
    if (Array.isArray(activeIds) && activeIds.some((id) => String(id) === trackedId)) {
      return TRACKED_CONTROL_PLANE_QUERY_REVALIDATE_MS;
    }
  } catch {}
  return CONTROL_PLANE_QUERY_REVALIDATE_MS;
}

function exactSelectorValue(value) {
  if (typeof value === 'string' || typeof value === 'number') return String(value);
  if (value && typeof value === 'object' && Object.keys(value).length === 1 && '$eq' in value) {
    return String(value.$eq || '');
  }
  return '';
}

function isMutableMembershipCollection(collectionName) {
  return collectionName === 'knowledge_tables';
}

async function invalidateByScanningQueryWindows(sidecar, collectionName, changedDocumentIds) {
  const all = await sidecar.backend.scanQueryWindows();
  const ids = new Set(changedDocumentIds.map((id) => String(id || '')).filter(Boolean));
  let invalidated = 0;
  for (const window of all) {
    if (window.collection !== collectionName) continue;
    const documentIds = Array.isArray(window.documentIds) ? window.documentIds : [];
    if (documentIds.some((id) => ids.has(String(id || '')))) {
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
}

function normalizeWindow(window, query) {
  if (window && typeof window === 'object') {
    return {
      offset: Math.max(0, Math.floor(Number(window.offset) || 0)),
      limit: Math.min(
        DEFAULT_WINDOW_LIMIT,
        Math.max(1, Math.floor(Number(window.limit) || DEFAULT_WINDOW_LIMIT)),
      ),
    };
  }
  return {
    offset: Math.max(0, Math.floor(Number(query?.skip) || 0)),
    limit: Math.min(
      DEFAULT_WINDOW_LIMIT,
      Math.max(1, Math.floor(Number(query?.limit) || DEFAULT_WINDOW_LIMIT)),
    ),
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

async function readLocalDocuments(storageCollection, query, window, documentIds = null) {
  if (Array.isArray(documentIds)) {
    if (documentIds.length === 0) return [];
    if (typeof storageCollection.findDocumentsById === 'function') {
      const documents = await storageCollection.findDocumentsById(documentIds);
      return documentIds
        .map((id) => documents?.[String(id)])
        .filter((document) => document && document._deleted !== true);
    }
    const memberIds = new Set(documentIds.map(String));
    const documents = typeof storageCollection.allDocuments === 'function'
      ? await storageCollection.allDocuments()
      : await storageCollection.queryDocuments(
        { selector: {}, skip: 0, limit: documentIds.length },
        { matchesSelector: defaultMatcher, sortDocuments: defaultSorter },
      );
    const byId = new Map(
      documents
        .filter((document) => memberIds.has(String(extractId(document))))
        .map((document) => [String(extractId(document)), document]),
    );
    return documentIds.map((id) => byId.get(String(id))).filter(Boolean);
  }
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

function authoritativeFetchedDocuments(documents, documentIds) {
  const byId = new Map(
    documents
      .filter((document) => document && document._deleted !== true)
      .map((document) => [String(extractId(document)), document]),
  );
  return documentIds.map((id) => byId.get(String(id))).filter(Boolean);
}

async function queryWindowDocumentsAvailable(storageCollection, documentIds) {
  if (!Array.isArray(documentIds) || documentIds.length === 0) return true;
  if (typeof storageCollection.findDocumentsById !== 'function') return true;
  const documents = await storageCollection.findDocumentsById(documentIds);
  return documentIds.every((id) => Boolean(documents?.[String(id)]));
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

function estimateBytesPerDocument(documents) {
  if (!Array.isArray(documents) || documents.length === 0) return 0;
  return Math.max(1, Math.ceil(estimateBytes(documents) / documents.length));
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

function createQueryCancelledError(reason) {
  const error = new Error(`QUERY_CANCELLED: ${reason}`);
  error.code = 'QUERY_CANCELLED';
  error.retryable = false;
  return error;
}

function throwIfQueryCancelled(invocationEntry) {
  if (!invocationEntry.cancelledReason) return;
  throw createQueryCancelledError(invocationEntry.cancelledReason);
}

function createQueryGenerationChangedError() {
  const error = new Error('QUERY_CANCELLED: generation-replaced');
  error.code = 'QUERY_CANCELLED';
  error.retryable = false;
  error.generationChanged = true;
  return error;
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
