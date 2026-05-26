// V1.5 sidecar metadata storage. Sits on top of a pluggable backend
// (in-memory for tests, IndexedDB for browser). Stores query-window
// completeness, per-document access times, and aggregate cache stats. The
// sidecar is *separate* from the primary documents IDB store; the primary DB
// remains byte-identical to V1.

import { createMemoryMetaBackend } from './query-meta-backend-memory.mjs';

export const SIDECAR_DATABASE_NAME = 'ctox_business_os_v1_5_meta';
export const SIDECAR_PIN_RECENT_READ_TTL_MS = 60_000;

const PIN_RECENT_READ = 'recently-read';

export class QueryMetaStorage {
  constructor(backend, { databaseName, clock = Date.now } = {}) {
    if (!backend) throw new TypeError('QueryMetaStorage requires a backend');
    if (!databaseName) throw new TypeError('QueryMetaStorage requires a databaseName');
    this.backend = backend;
    this.databaseName = databaseName;
    this.clock = clock;
  }

  async getQueryWindow(key) {
    const record = await this.backend.getQueryWindow(stringKey(key));
    if (!record) return null;
    record.lastAccessedAt = this.clock();
    await this.backend.putQueryWindow(record);
    return record;
  }

  async upsertQueryWindow({ collection, queryFingerprint, offset, limit, documentIds, complete, authoritativeRevision }) {
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
      authoritativeRevision: authoritativeRevision ?? null,
      createdAt: existing?.createdAt ?? now,
      updatedAt: now,
      lastAccessedAt: now,
    };
    await this.backend.putQueryWindow(record);
    return record;
  }

  async invalidateQueryWindow(key) {
    const stringified = stringKey(key);
    const existing = await this.backend.getQueryWindow(stringified);
    if (!existing) return;
    existing.complete = false;
    existing.updatedAt = this.clock();
    await this.backend.putQueryWindow(existing);
  }

  async touchDocuments(collection, ids, { estimatedBytes = 0, pinReason = PIN_RECENT_READ } = {}) {
    const now = this.clock();
    for (const id of ids) {
      const previous = (await this.backend.getDocumentAccess(collection, id)) || {};
      await this.backend.putDocumentAccess({
        collection,
        id,
        lastAccessedAt: now,
        pinReason: previous.dirty ? 'dirty' : pinReason,
        dirty: Boolean(previous.dirty),
        estimatedBytes: estimatedBytes || previous.estimatedBytes || 0,
      });
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

  async close() {
    await this.backend.close();
  }

  /// Evicts LRU document access entries until the working set fits the budget.
  /// Skips dirty docs and unexpired recently-read pins. Returns the number of
  /// document records removed.
  async runEvictionIfOverBudget() {
    const stats = await this.getCacheStats();
    if (!stats.budgetBytes || stats.estimatedBytes <= stats.budgetBytes) {
      return 0;
    }
    const all = await this.backend.scanDocumentAccess();
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
    let remainingBytes = stats.estimatedBytes;
    for (const candidate of candidates) {
      if (remainingBytes <= stats.budgetBytes) break;
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
      await this.runEvictionIfOverBudget();
      try {
        return await writeFn();
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
  startEvictionScheduler({ intervalMs = 30_000 } = {}) {
    if (this._evictionTimer) return { stop: () => this.stopEvictionScheduler() };
    this._evictionTimer = setInterval(() => {
      this.runEvictionIfOverBudget().catch(() => {});
    }, intervalMs);
    if (typeof this._evictionTimer.unref === 'function') {
      this._evictionTimer.unref();
    }
    return { stop: () => this.stopEvictionScheduler() };
  }

  stopEvictionScheduler() {
    if (this._evictionTimer) {
      clearInterval(this._evictionTimer);
      this._evictionTimer = null;
    }
  }

  /// Orphan-window GC: drop sidecar query-window entries that haven't been
  /// read in `maxAgeMs` milliseconds (default 7 days). Documents referenced
  /// by other windows remain. This keeps the sidecar from growing monotonically
  /// as one-off queries accumulate.
  async runWindowGc({ maxAgeMs = 7 * 24 * 60 * 60 * 1000 } = {}) {
    const now = this.clock();
    const all = await this.backend.scanQueryWindows();
    let removed = 0;
    for (const window of all) {
      const age = now - (window.lastAccessedAt ?? window.updatedAt ?? window.createdAt ?? now);
      if (age >= maxAgeMs) {
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
