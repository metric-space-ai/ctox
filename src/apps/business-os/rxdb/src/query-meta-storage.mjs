// V1.5 sidecar metadata storage. Sits on top of a pluggable backend
// (in-memory for tests, IndexedDB for browser). Stores query-window
// completeness, per-document access times, and aggregate cache stats. The
// sidecar is *separate* from the primary documents IDB store; the primary DB
// remains byte-identical to V1.

import { createMemoryMetaBackend } from './query-meta-backend-memory.mjs';

export const SIDECAR_DATABASE_NAME = 'ctox_business_os_v1_5_meta';
export const SIDECAR_PIN_RECENT_READ_TTL_MS = 60_000;

const PIN_RECENT_READ = 'recently-read';
const evictionSchedulerGroups = new Map();

export class QueryMetaStorage {
  constructor(backend, {
    databaseName,
    schedulerKey = databaseName,
    clock = Date.now,
    primaryDelete = null,
  } = {}) {
    if (!backend) throw new TypeError('QueryMetaStorage requires a backend');
    if (!databaseName) throw new TypeError('QueryMetaStorage requires a databaseName');
    this.backend = backend;
    this.databaseName = databaseName;
    this.schedulerKey = schedulerKey;
    this.clock = clock;
    // V1.5 production hardening: primaryDelete(collection, id) actually
    // removes the document from the primary IndexedDB `documents` store.
    // Without it, evictDocuments only clears sidecar metadata and the
    // primary cache grows unbounded — which is the bug the review caught.
    this.primaryDelete = typeof primaryDelete === 'function' ? primaryDelete : null;
  }

  setPrimaryDelete(fn) {
    this.primaryDelete = typeof fn === 'function' ? fn : null;
  }

  async getQueryWindow(key) {
    const record = await this.backend.getQueryWindow(stringKey(key));
    if (!record) return null;
    record.lastAccessedAt = this.clock();
    await this.backend.putQueryWindow(record);
    return record;
  }

  async upsertQueryWindow({ collection, queryFingerprint, offset, limit, documentIds, complete, authoritativeRevision, queryShape = null }) {
    const now = this.clock();
    const existing = await this.backend.getQueryWindow(
      [collection, queryFingerprint, offset, limit].join('|'),
    );
    const record = {
      collection,
      queryFingerprint,
      offset,
      limit,
      documentIds: [...documentIds],
      complete: Boolean(complete),
      // Sticky marker: once a window has been complete, its member documents
      // exist in the primary store (and replication keeps them fresh). The
      // demand loader serves such windows local-first while it revalidates
      // in the background, and the reconnect-abort path must NOT tombstone
      // their members as partial orphans.
      everCompleted: Boolean(complete) || Boolean(existing?.everCompleted),
      authoritativeRevision: authoritativeRevision ?? null,
      queryShape: queryShape && typeof queryShape === 'object' ? structuredCloneSafe(queryShape) : null,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
      lastAccessedAt: now,
    };
    await this.backend.putQueryWindow(record);
    // SYNC-52: the change-invalidation path routes through the
    // collection_documentId ref index instead of a full window scan. Real
    // document ids cover member-invalidation; two synthetic ref ids cover the
    // rest without materializing every window on each replicated change:
    //   `$nonsimple`      — a window whose result cannot be reasoned about
    //                       from a single doc (sorts / non-equality selectors)
    //                       and must be invalidated on ANY change.
    //   `$field|<path>`   — a simple-equality window keyed on <path>; a change
    //                       touching that field may newly enter/leave it.
    // These live in the SAME ref store (keyed by windowKey) so they are cleared
    // whenever the window is replaced or deleted. They are attached only to the
    // record handed to the ref writer, not persisted on the window itself.
    await this.backend.replaceQueryWindowDocumentRefs?.({
      ...record,
      selectorRefIds: computeSelectorRefIds(queryShape),
    });
    return record;
  }

  async invalidateQueryWindow(key) {
    const stringified = stringKey(key);
    const existing = await this.backend.getQueryWindow(stringified);
    if (!existing) return;
    existing.everCompleted = Boolean(existing.everCompleted) || Boolean(existing.complete);
    existing.complete = false;
    existing.updatedAt = this.clock();
    await this.backend.putQueryWindow(existing);
  }

  async touchDocuments(collection, ids, { estimatedBytes = 0, pinReason = PIN_RECENT_READ } = {}) {
    const now = this.clock();
    const normalizedIds = Array.isArray(ids) ? ids.filter(Boolean) : [];
    if (!normalizedIds.length) return;
    const perDocumentBytes = normalizeEstimatedBytes(estimatedBytes);
    let deltaBytes = 0;
    for (const id of normalizedIds) {
      const previous = (await this.backend.getDocumentAccess(collection, id)) || {};
      const nextEstimatedBytes = perDocumentBytes || previous.estimatedBytes || 0;
      deltaBytes += nextEstimatedBytes - (previous.estimatedBytes || 0);
      await this.backend.putDocumentAccess({
        collection,
        id,
        lastAccessedAt: now,
        pinReason: previous.dirty ? 'dirty' : pinReason,
        dirty: Boolean(previous.dirty),
        estimatedBytes: nextEstimatedBytes,
      });
    }
    if (deltaBytes !== 0) {
      const stats = await this.getCacheStats();
      stats.estimatedBytes = Math.max(0, (stats.estimatedBytes || 0) + deltaBytes);
      await this.backend.putCacheStats(stats);
    }
  }

  async markDirty(collection, id, dirty) {
    const previous = (await this.backend.getDocumentAccess(collection, id)) || {
      collection,
      id,
      lastAccessedAt: this.clock(),
      estimatedBytes: 0,
    };
    await this.backend.putDocumentAccess({
      ...previous,
      dirty: Boolean(dirty),
      pinReason: dirty ? 'dirty' : previous.pinReason ?? null,
    });
  }

  async getDocumentAccess(collection, id) {
    const record = await this.backend.getDocumentAccess(collection, id);
    return record ? { ...record } : null;
  }

  async evictDocuments(ids) {
    const now = this.clock();
    let removed = 0;
    for (const { collection, id } of ids) {
      const record = await this.backend.getDocumentAccess(collection, id);
      if (!record) continue;
      if (record.dirty) continue;
      if (record.pinReason === PIN_RECENT_READ && now - record.lastAccessedAt < SIDECAR_PIN_RECENT_READ_TTL_MS) {
        continue;
      }
      // Remove from the PRIMARY documents store first. If that fails the
      // metadata stays so we don't lose track of the doc on the next pass.
      if (this.primaryDelete) {
        try {
          await this.primaryDelete(collection, id);
        } catch {
          // Primary-store delete failed; skip this doc to avoid orphan
          // metadata while the primary copy is still present.
          continue;
        }
      }
      await this.backend.deleteDocumentAccess(collection, id);
      removed += 1;
    }
    const stats = (await this.backend.getCacheStats(this.databaseName)) || {
      databaseName: this.databaseName,
      estimatedBytes: 0,
      budgetBytes: 0,
      lastEvictionAt: null,
    };
    stats.lastEvictionAt = removed > 0 ? now : stats.lastEvictionAt;
    stats.estimatedBytes = await this.estimateWorkingSetBytes();
    await this.backend.putCacheStats(stats);
    return removed;
  }

  async estimateWorkingSetBytes() {
    const docs = await this.backend.scanDocumentAccess();
    return docs.reduce((sum, record) => sum + (record.estimatedBytes || 0), 0);
  }

  async setBudgetBytes(budgetBytes) {
    const stats = (await this.backend.getCacheStats(this.databaseName)) || {
      databaseName: this.databaseName,
      estimatedBytes: 0,
      budgetBytes: 0,
      lastEvictionAt: null,
    };
    stats.budgetBytes = Number(budgetBytes) || 0;
    await this.backend.putCacheStats(stats);
  }

  async getCacheStats() {
    return (
      (await this.backend.getCacheStats(this.databaseName)) || {
        databaseName: this.databaseName,
        estimatedBytes: 0,
        budgetBytes: 0,
        lastEvictionAt: null,
      }
    );
  }

  async clear() {
    await this.backend.clear();
  }

  async invalidateQueryWindowsForDocuments(collection, ids) {
    const normalizedIds = normalizeDocumentIds(ids);
    if (!collection || !normalizedIds.length) return 0;
    const windowKeys = typeof this.backend.getQueryWindowKeysByDocumentIds === 'function'
      ? await this.backend.getQueryWindowKeysByDocumentIds(collection, normalizedIds)
      : await this.scanQueryWindowKeysForDocuments(collection, normalizedIds);
    let invalidated = 0;
    const seen = new Set();
    for (const key of windowKeys) {
      const stringified = stringKey(key);
      if (seen.has(stringified)) continue;
      seen.add(stringified);
      const window = await this.backend.getQueryWindow(stringified);
      if (!window || window.collection !== collection) continue;
      await this.invalidateQueryWindow([
        window.collection,
        window.queryFingerprint,
        window.offset,
        window.limit,
      ]);
      invalidated += 1;
    }
    return invalidated;
  }

  async invalidateQueryWindowsForChanges(collection, documents, primaryPath = 'id') {
    const changes = Array.isArray(documents) ? documents.filter(Boolean) : [];
    if (!collection || !changes.length) return 0;
    // SYNC-52: prefer the collection_documentId ref index so a change batch is
    // O(matching refs), not O(all windows). The old path called
    // `scanQueryWindows()` (an IndexedDB `getAll`) unconditionally on every
    // replicated change tick, materializing the entire — unbounded — window
    // store (with each window's full documentId array) into RAM. Backends
    // without the ref index still fall back to the whole-store scan.
    if (typeof this.backend.getQueryWindowKeysByDocumentIds !== 'function') {
      return this.scanInvalidateQueryWindowsForChanges(collection, changes, primaryPath);
    }
    const lookupIds = new Set(['$nonsimple']);
    for (const document of changes) {
      const id = valueAtPath(document, primaryPath);
      if (id != null && id !== '') lookupIds.add(String(id));
      for (const path of documentLeafPaths(document)) lookupIds.add(`$field|${path}`);
    }
    const windowKeys = await this.backend.getQueryWindowKeysByDocumentIds(collection, [...lookupIds]);
    let invalidated = 0;
    const seen = new Set();
    for (const key of windowKeys) {
      const stringified = stringKey(key);
      if (seen.has(stringified)) continue;
      seen.add(stringified);
      const window = await this.backend.getQueryWindow(stringified);
      if (!window || window.collection !== collection) continue;
      // Re-verify precisely — identical predicate to the legacy full scan, so
      // over-matched candidates (multi-field equality windows found via one
      // field, wrong equality value, etc.) are filtered out and correctness is
      // preserved: a change still invalidates every window whose result could
      // include the changed doc, and nothing more.
      if (!changeAffectsWindow(window, changes, primaryPath)) continue;
      await this.invalidateQueryWindow([
        window.collection,
        window.queryFingerprint,
        window.offset,
        window.limit,
      ]);
      invalidated += 1;
    }
    return invalidated;
  }

  // Whole-store fallback for backends without the document-ref index. Keeps the
  // original semantics for the in-memory/degraded path.
  async scanInvalidateQueryWindowsForChanges(collection, changes, primaryPath = 'id') {
    const all = await this.backend.scanQueryWindows();
    let invalidated = 0;
    for (const window of all) {
      if (window.collection !== collection) continue;
      if (!changeAffectsWindow(window, changes, primaryPath)) continue;
      await this.invalidateQueryWindow([
        window.collection,
        window.queryFingerprint,
        window.offset,
        window.limit,
      ]);
      invalidated += 1;
    }
    return invalidated;
  }

  async scanQueryWindowKeysForDocuments(collection, ids) {
    const idSet = new Set(ids);
    const all = await this.backend.scanQueryWindows();
    const keys = [];
    for (const window of all) {
      if (window.collection !== collection) continue;
      const documentIds = Array.isArray(window.documentIds) ? window.documentIds : [];
      if (!documentIds.some((id) => idSet.has(String(id || '')))) continue;
      keys.push([
        window.collection,
        window.queryFingerprint,
        window.offset,
        window.limit,
      ]);
    }
    return keys;
  }

  async close() {
    await this.backend.close();
  }

  /// Evicts LRU document access entries until the working set fits the budget.
  /// Skips dirty docs and unexpired recently-read pins. Returns the number of
  /// document records removed.
  async runEvictionIfOverBudget({ forceRecount = false } = {}) {
    const stats = await this.getCacheStats();
    if (!stats.budgetBytes) {
      return 0;
    }
    if (!forceRecount && (stats.estimatedBytes || 0) <= stats.budgetBytes) {
      return 0;
    }

    const all = await this.backend.scanDocumentAccess();
    const workingSetBytes = sumEstimatedDocumentAccessBytes(all);
    if (stats.estimatedBytes !== workingSetBytes) {
      stats.estimatedBytes = workingSetBytes;
      await this.backend.putCacheStats(stats);
    }
    if (workingSetBytes <= stats.budgetBytes) {
      return 0;
    }
    const now = this.clock();
    // Sort oldest access first; skip dirty/pinned.
    const candidates = all
      .filter((record) => !record.dirty)
      .filter((record) => {
        if (record.pinReason !== 'recently-read') return true;
        return now - record.lastAccessedAt >= SIDECAR_PIN_RECENT_READ_TTL_MS;
      })
      .sort((a, b) => a.lastAccessedAt - b.lastAccessedAt);
    let removed = 0;
    let remainingBytes = workingSetBytes;
    for (const candidate of candidates) {
      if (remainingBytes <= stats.budgetBytes) break;
      if (this.primaryDelete) {
        try {
          await this.primaryDelete(candidate.collection, candidate.id);
        } catch {
          continue;
        }
      }
      await this.backend.deleteDocumentAccess(candidate.collection, candidate.id);
      remainingBytes -= candidate.estimatedBytes || 0;
      removed += 1;
    }
    if (removed > 0) {
      const updated = { ...stats, estimatedBytes: remainingBytes, lastEvictionAt: now };
      await this.backend.putCacheStats(updated);
    }
    return removed;
  }

  async recordEstimatedBytes(bytes) {
    const stats = await this.getCacheStats();
    stats.estimatedBytes = Math.max(0, Number(bytes) || 0);
    await this.backend.putCacheStats(stats);
  }

  /// Wraps an IDB write attempt in a quota-recovery loop. On
  /// `QuotaExceededError` we run eviction once and retry; on second failure
  /// the error propagates. Use this from production paths that materialize
  /// fetched chunks into the primary store.
  async withQuotaRecovery(writeFn) {
    try {
      return await writeFn();
    } catch (err) {
      if (!isQuotaExceeded(err)) throw err;
      const stats = await this.getCacheStats();
      // Force aggressive eviction: target half of current budget.
      const tighten = Math.max(1024, Math.floor((stats.budgetBytes || stats.estimatedBytes || 65536) / 2));
      await this.setBudgetBytes(tighten);
      await this.runEvictionIfOverBudget({ forceRecount: true });
      try {
        const result = await writeFn();
        if (stats.budgetBytes) await this.setBudgetBytes(stats.budgetBytes);
        return result;
      } catch (retryErr) {
        // Restore budget for visibility; rethrow.
        if (stats.budgetBytes) await this.setBudgetBytes(stats.budgetBytes);
        throw retryErr;
      }
    }
  }

  /// Starts a periodic eviction scheduler. The handle returned has a
  /// `stop()` method. Idempotent: calling twice with the same handle is
  /// safe. Default interval: 30s.
  startEvictionScheduler({
    intervalMs = 30_000,
    globalBudgetBytes = 0,
    shareBudgetBytes = 0,
  } = {}) {
    if (this._evictionSchedulerGroupKey) {
      return { stop: () => this.stopEvictionScheduler() };
    }
    const key = String(this.schedulerKey || this.databaseName);
    let group = evictionSchedulerGroups.get(key);
    if (!group) {
      group = {
        storages: new Set(),
        timer: null,
        intervalMs,
        globalBudgetBytes: Math.max(0, Number(globalBudgetBytes) || 0),
      };
      evictionSchedulerGroups.set(key, group);
    }
    group.globalBudgetBytes = Math.max(
      group.globalBudgetBytes,
      Math.max(0, Number(globalBudgetBytes) || 0),
    );
    this._configuredShareBudgetBytes = Math.max(0, Number(shareBudgetBytes) || 0);
    this._evictionSchedulerGroupKey = key;
    group.storages.add(this);
    rebalanceEvictionSchedulerGroup(group).catch(() => {});
    if (!group.timer) {
      group.timer = setInterval(() => runEvictionSchedulerGroup(group), group.intervalMs);
      if (typeof group.timer.unref === 'function') group.timer.unref();
    }
    return { stop: () => this.stopEvictionScheduler() };
  }

  stopEvictionScheduler() {
    const key = this._evictionSchedulerGroupKey;
    if (!key) return;
    this._evictionSchedulerGroupKey = null;
    const group = evictionSchedulerGroups.get(key);
    if (!group) return;
    group.storages.delete(this);
    if (group.storages.size === 0) {
      if (group.timer) clearInterval(group.timer);
      evictionSchedulerGroups.delete(key);
      return;
    }
    rebalanceEvictionSchedulerGroup(group).catch(() => {});
  }

  /// Orphan-window GC: hard-delete sidecar query-window entries (and their
  /// document/selector refs — `deleteQueryWindow` cascades) that have aged out.
  /// Two thresholds, because one-off queries with a varying selector value mint
  /// a fresh window on every exec:
  ///   - complete / ever-completed windows: `maxAgeMs` (default 7 days). These
  ///     are served local-first while revalidating (the `everCompleted` flag is
  ///     load-bearing for stale-while-revalidate, see correctness-reconnect),
  ///     so they get the full window.
  ///   - windows that were invalidated/minted but NEVER completed (pure
  ///     tombstones, no local-first value): `staleIncompleteMaxAgeMs` (default
  ///     1 hour). This is the "short grace" hard-delete for sticky tombstones.
  /// Wired into the production eviction scheduler via `runSchedulerMaintenance`.
  async runWindowGc({
    maxAgeMs = 7 * 24 * 60 * 60 * 1000,
    staleIncompleteMaxAgeMs = 60 * 60 * 1000,
  } = {}) {
    const now = this.clock();
    const all = await this.backend.scanQueryWindows();
    let removed = 0;
    for (const window of all) {
      const age = now - (window.lastAccessedAt ?? window.updatedAt ?? window.createdAt ?? now);
      const servesLocalFirst = Boolean(window.complete) || Boolean(window.everCompleted);
      const threshold = servesLocalFirst ? maxAgeMs : staleIncompleteMaxAgeMs;
      if (age >= threshold) {
        await this.backend.deleteQueryWindow([
          window.collection,
          window.queryFingerprint,
          window.offset,
          window.limit,
        ]);
        removed += 1;
      }
    }
    return removed;
  }

  /// Periodic maintenance run by the eviction scheduler timer (and callable
  /// directly in tests). Evicts over-budget documents AND reclaims aged-out
  /// query windows — the latter is the only production caller of `runWindowGc`,
  /// which previously ran from tests alone while the sidecar grew unbounded.
  async runSchedulerMaintenance() {
    const evicted = await this.runEvictionIfOverBudget().catch(() => 0);
    const windowsReclaimed = await this.runWindowGc().catch(() => 0);
    return { evicted, windowsReclaimed };
  }
}

function changeAffectsWindow(window, changes, primaryPath) {
  const members = new Set((window.documentIds || []).map(String));
  const simple = simpleEqualitySelector(window.queryShape);
  if (!simple) return true;
  return changes.some((document) => {
    const id = valueAtPath(document, primaryPath);
    return members.has(String(id ?? '')) || matchesSimpleEquality(document, simple);
  });
}

// SYNC-52: synthetic ref ids that let a change be resolved to affected windows
// through the document-ref index (see upsertQueryWindow). A non-simple window
// is invalidated by any change (`$nonsimple`); a simple-equality window is a
// candidate when a change touches one of its selector fields (`$field|<path>`).
function computeSelectorRefIds(queryShape) {
  const simple = simpleEqualitySelector(queryShape);
  if (!simple) return ['$nonsimple'];
  return simple.map(([field]) => `$field|${field}`);
}

// Flatten a changed document to the set of dotted leaf paths it defines, so the
// change path can look up `$field|<path>` candidates. Mirrors `valueAtPath`
// (dot-separated). `_meta` is skipped (large HLC/origin blob, never a selector
// target); depth is bounded defensively.
function documentLeafPaths(document, prefix = '', out = new Set(), depth = 0) {
  if (!document || typeof document !== 'object' || Array.isArray(document) || depth > 6) return out;
  for (const [key, value] of Object.entries(document)) {
    if (key === '_meta') continue;
    const path = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      out.add(path);
      documentLeafPaths(value, path, out, depth + 1);
    } else {
      out.add(path);
    }
  }
  return out;
}

function simpleEqualitySelector(queryShape) {
  if (!queryShape || typeof queryShape !== 'object') return null;
  if (Array.isArray(queryShape.sort) && queryShape.sort.length > 0) return null;
  const selector = queryShape.selector;
  if (!selector || typeof selector !== 'object' || Array.isArray(selector)) return null;
  const entries = Object.entries(selector);
  if (!entries.length) return null;
  const equalities = [];
  for (const [field, condition] of entries) {
    if (!field || field.startsWith('$')) return null;
    if (condition && typeof condition === 'object') {
      const keys = Object.keys(condition);
      if (keys.length !== 1 || keys[0] !== '$eq') return null;
      equalities.push([field, condition.$eq]);
    } else {
      equalities.push([field, condition]);
    }
  }
  return equalities;
}

function matchesSimpleEquality(document, equalities) {
  if (!equalities || document?._deleted) return false;
  return equalities.every(([field, expected]) => Object.is(valueAtPath(document, field), expected));
}

function valueAtPath(value, path) {
  return String(path || '').split('.').filter(Boolean)
    .reduce((current, segment) => current?.[segment], value);
}

function structuredCloneSafe(value) {
  try { return globalThis.structuredClone?.(value) ?? JSON.parse(JSON.stringify(value)); }
  catch { return null; }
}

async function rebalanceEvictionSchedulerGroup(group) {
  const storages = [...group.storages];
  if (!storages.length) return;
  const globalShare = group.globalBudgetBytes > 0
    ? Math.max(1, Math.floor(group.globalBudgetBytes / storages.length))
    : Number.POSITIVE_INFINITY;
  await Promise.all(storages.map(async (storage) => {
    const configured = storage._configuredShareBudgetBytes || globalShare;
    const effective = Math.max(1, Math.floor(Math.min(configured, globalShare)));
    await storage.setBudgetBytes(effective);
  }));
}

async function runEvictionSchedulerGroup(group) {
  await rebalanceEvictionSchedulerGroup(group);
  await Promise.all(
    // SYNC-52: run full maintenance (eviction AND window GC) on each tick so
    // orphan query windows are actually reclaimed in production.
    [...group.storages].map((storage) => storage.runSchedulerMaintenance().catch(() => 0)),
  );
}

// Database-wide quota coordinator used by the primary store and the recovery
// journal. It temporarily tightens every registered sidecar share, evicts only
// clean/unpinned replicated rows, then restores the configured group budgets.
// If no sidecar is registered yet this is a safe no-op; the caller's retry
// remains authoritative and will surface a typed quota error if storage is
// still full.
export async function recoverQueryMetaQuota(schedulerKey) {
  const group = evictionSchedulerGroups.get(String(schedulerKey || ''));
  if (!group?.storages?.size) return { evicted: 0, storages: 0 };
  const storages = [...group.storages];
  const previous = await Promise.all(storages.map((storage) => storage.getCacheStats()));
  let evicted = 0;
  try {
    for (let index = 0; index < storages.length; index += 1) {
      const storage = storages[index];
      const stats = previous[index];
      const tightened = Math.max(1024, Math.floor((stats.budgetBytes || stats.estimatedBytes || 65536) / 2));
      await storage.setBudgetBytes(tightened);
      evicted += await storage.runEvictionIfOverBudget({ forceRecount: true });
    }
  } finally {
    await rebalanceEvictionSchedulerGroup(group);
  }
  return { evicted, storages: storages.length };
}

function normalizeEstimatedBytes(estimatedBytes) {
  const bytes = Math.max(0, Number(estimatedBytes) || 0);
  return bytes > 0 ? Math.max(1, Math.ceil(bytes)) : 0;
}

function sumEstimatedDocumentAccessBytes(records) {
  return (Array.isArray(records) ? records : []).reduce(
    (sum, record) => sum + (record.estimatedBytes || 0),
    0,
  );
}

function normalizeDocumentIds(ids) {
  if (!Array.isArray(ids)) return [];
  return Array.from(new Set(ids.map((id) => String(id || '')).filter(Boolean)));
}

function isQuotaExceeded(err) {
  if (!err) return false;
  if (err.name === 'QuotaExceededError') return true;
  if (typeof err.code === 'number' && err.code === 22) return true; // legacy
  const msg = String(err.message || '').toLowerCase();
  return msg.includes('quota') || msg.includes('storage full');
}

export function createSidecarWithMemoryBackend({ databaseName = SIDECAR_DATABASE_NAME, clock = Date.now } = {}) {
  return new QueryMetaStorage(createMemoryMetaBackend(), { databaseName, clock });
}

function stringKey(key) {
  if (Array.isArray(key)) return key.join('|');
  if (typeof key === 'string') return key;
  throw new TypeError('query window key must be array or string');
}
