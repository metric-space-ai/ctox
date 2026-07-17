import {
  decryptRecoveryArtifact,
  encryptRecoveryArtifact,
  sha256Json,
} from './recovery-crypto.mjs';

const JOURNAL_VERSION = 3;
const BATCH_STORE = 'batches';
const BATCH_STATE_COLLECTION_INDEX = 'stateCollection';
const CONFLICT_STORE = 'conflicts';
const META_STORE = 'meta';
const ACKED_RETENTION_MS = 24 * 60 * 60 * 1000;
const previews = new Map();

export async function openRecoveryJournal({ databaseName, instanceId = databaseName, quotaCoordinator = null } = {}) {
  if (!databaseName) throw new TypeError('Recovery journal requires databaseName');
  const db = await openJournalDatabase(`${databaseName}__recovery_v2`);
  return new CtoxRecoveryJournal(db, { databaseName, instanceId, quotaCoordinator });
}

export class CtoxRecoveryJournal {
  constructor(db, { databaseName, instanceId, quotaCoordinator }) {
    this.db = db;
    this.databaseName = databaseName;
    this.instanceId = instanceId;
    this.quotaCoordinator = quotaCoordinator;
    this.replayers = new Map();
  }

  registerCollection(collection, { schemaHash = '', applyBatch, resolveConflict = null } = {}) {
    if (!collection || typeof applyBatch !== 'function') return;
    this.replayers.set(collection, { schemaHash, applyBatch, resolveConflict });
  }

  async appendBatch({ collection, schemaHash = '', primaryPath = 'id', operation = 'write', rows = [], baseById = null }) {
    const normalizedRows = structuredCloneSafe(rows);
    const batch = {
      batchId: globalThis.crypto?.randomUUID?.() || `batch-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      sequence: 0,
      schema: 'ctox.indexeddb.recovery-journal.v2',
      databaseName: this.databaseName,
      instanceId: this.instanceId,
      collection,
      schemaHash,
      primaryPath,
      operation,
      rows: normalizedRows,
      baseById: structuredCloneSafe(baseById),
      documentIds: normalizedRows.map((row) => documentId(row?.document || row, primaryPath)).filter(Boolean),
      committedDocs: {},
      ackedIds: [],
      payloadHash: await sha256Json({ collection, schemaHash, primaryPath, operation, rows: normalizedRows, baseById }),
      state: 'pending',
      createdAtMs: Date.now(),
      primaryCommittedAtMs: 0,
      masterAckedAtMs: 0,
    };
    await this.withQuotaRecovery(() => putSequencedBatch(this.db, batch));
    await this.publishStatus();
    return batch.batchId;
  }

  async commitBatch(batchId, success = {}) {
    await updateRecord(this.db, BATCH_STORE, batchId, (batch) => ({
      ...batch,
      committedDocs: structuredCloneSafe(success),
      primaryCommittedAtMs: Date.now(),
    }));
    await this.publishStatus();
  }

  async markMasterAcknowledged(collection, documents = {}) {
    const batches = await this.listBatches('pending', collection);
    for (const batch of batches) {
      if (batch.collection !== collection) continue;
      const acked = new Set(batch.ackedIds || []);
      for (const id of batch.documentIds || []) {
        const master = documents[id];
        const local = batch.committedDocs?.[id];
        if (master && local && masterAcknowledgesLocal(master, local, collection)) acked.add(id);
      }
      const complete = (batch.documentIds || []).every((id) => acked.has(id));
      await updateRecord(this.db, BATCH_STORE, batch.batchId, (current) => ({
        ...current,
        ackedIds: [...acked],
        state: complete ? 'master_acked' : 'pending',
        masterAckedAtMs: complete ? Date.now() : 0,
      }));
    }
    await this.gc();
    await this.publishStatus();
  }

  // SYNC-40: force-acknowledge local writes the native peer terminally REJECTED
  // (authz/schema), not ones it accepted. `markMasterAcknowledged` only clears a
  // WAL entry when the master row acknowledges the local content — a denied
  // write never round-trips, so without this its batch stays pending and
  // replays as a fresh pushable write on every restart (re-push → re-deny →
  // re-journal). The rejected version is preserved in the conflict store; here
  // we drop it from the pending write-ahead log so it stops being re-pushed.
  async markReconciled(collection, ids = []) {
    const idSet = new Set((Array.isArray(ids) ? ids : []).map((id) => String(id)));
    if (!idSet.size) return;
    const batches = await this.listBatches('pending', collection);
    for (const batch of batches) {
      if (batch.collection !== collection) continue;
      const relevant = (batch.documentIds || []).filter((id) => idSet.has(String(id)));
      if (!relevant.length) continue;
      const acked = new Set(batch.ackedIds || []);
      for (const id of relevant) acked.add(id);
      const complete = (batch.documentIds || []).every((id) => acked.has(id));
      await updateRecord(this.db, BATCH_STORE, batch.batchId, (current) => ({
        ...current,
        ackedIds: [...acked],
        state: complete ? 'master_acked' : 'pending',
        masterAckedAtMs: complete ? Date.now() : (current.masterAckedAtMs || 0),
      }));
    }
    await this.gc();
    await this.publishStatus();
  }

  async replayRegisteredCollections(collection = null) {
    const batches = await this.listBatches('pending', collection);
    const outcomes = [];
    for (const batch of batches) {
      // A primary-committed batch is durable already and only waits for the
      // native peer acknowledgement. Replaying it on every collection
      // registration multiplied startup work by collections × journal size.
      if (Number(batch.primaryCommittedAtMs || 0) > 0) continue;
      const replayer = this.replayers.get(batch.collection);
      if (!replayer) continue;
      if (batch.schemaHash && replayer.schemaHash && batch.schemaHash !== replayer.schemaHash) {
        await this.recordConflict({
          code: 'recovery_schema_mismatch',
          collection: batch.collection,
          batchId: batch.batchId,
          base: batch.baseById,
          local: batch.rows,
          master: null,
        });
        await updateRecord(this.db, BATCH_STORE, batch.batchId, (current) => ({
          ...current,
          state: 'conflict',
          conflictAtMs: Date.now(),
        }));
        outcomes.push({ batchId: batch.batchId, status: 'conflict' });
        continue;
      }
      try {
        const result = await replayer.applyBatch(batch);
        await this.commitBatch(batch.batchId, result?.success || {});
        outcomes.push({ batchId: batch.batchId, status: 'replayed' });
      } catch (error) {
        await this.recordConflict({
          code: error?.code || 'recovery_replay_failed',
          collection: batch.collection,
          batchId: batch.batchId,
          base: batch.baseById,
          local: batch.rows,
          master: null,
          message: error?.message || String(error),
        });
        await updateRecord(this.db, BATCH_STORE, batch.batchId, (current) => ({
          ...current,
          state: 'conflict',
          conflictAtMs: Date.now(),
        }));
        outcomes.push({ batchId: batch.batchId, status: 'conflict' });
      }
    }
    await this.publishStatus();
    return outcomes;
  }

  async recordConflict(conflict = {}) {
    const record = {
      conflictId: conflict.conflictId || globalThis.crypto?.randomUUID?.() || `conflict-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      schema: 'ctox.indexeddb.recovery-conflict.v1',
      databaseName: this.databaseName,
      instanceId: this.instanceId,
      state: 'pending',
      createdAtMs: Date.now(),
      ...structuredCloneSafe(conflict),
    };
    await this.withQuotaRecovery(() => putRecord(this.db, CONFLICT_STORE, record));
    await this.publishStatus();
    return record;
  }

  async listConflicts() {
    return getAllRecords(this.db, CONFLICT_STORE).then((rows) => rows.filter((row) => row.state === 'pending'));
  }

  async resolveConflict(conflictId, resolution) {
    if (!['keep_local', 'keep_master', 'restore_as_copy'].includes(resolution)) {
      throw recoveryError('structured_conflict_requires_resolution', `Unsupported conflict resolution ${resolution}`);
    }
    const conflict = await getRecord(this.db, CONFLICT_STORE, conflictId);
    if (!conflict) return false;
    if (conflict.conflictType === 'delete_vs_update' && resolution === 'keep_local') {
      throw recoveryError(
        'structured_conflict_requires_resolution',
        'A native tombstone is authoritative. Restore the local version as a copy instead.',
      );
    }
    const replayer = this.replayers.get(conflict.collection);
    if ((resolution === 'keep_local' || resolution === 'restore_as_copy') && !replayer) {
      throw recoveryError('structured_conflict_requires_resolution', `Collection ${conflict.collection} is not registered.`);
    }
    if (resolution === 'keep_local') {
      const rows = normalizeConflictRows(conflict.local);
      await (replayer.resolveConflict || replayer.applyBatch)({
        operation: 'write',
        rows,
        baseById: conflictBaseById(conflict, rows),
      });
    } else if (resolution === 'restore_as_copy') {
      const rows = normalizeConflictRows(conflict.local).map((row) => restoreAsCopy(row?.document || row));
      await (replayer.resolveConflict || replayer.applyBatch)({ operation: 'write', rows, baseById: null });
    }
    await updateRecord(this.db, CONFLICT_STORE, conflictId, (current) => ({
      ...current,
      state: 'resolved',
      resolution,
      resolvedAtMs: Date.now(),
    }));
    await this.publishStatus();
    return true;
  }

  async getStatus() {
    const batches = await this.listBatches('pending');
    const conflicts = await this.listConflicts();
    const bytes = estimateBytes(batches) + estimateBytes(conflicts);
    return {
      schema: 'ctox.browser-recovery.status.v2',
      databaseName: this.databaseName,
      instanceId: this.instanceId,
      pendingBatches: batches.length,
      pendingWrites: batches.reduce((sum, batch) => sum + (batch.documentIds?.length || 0), 0),
      pendingBytes: bytes,
      oldestPendingAtMs: batches.reduce((oldest, batch) => Math.min(oldest, batch.createdAtMs || oldest), Number.MAX_SAFE_INTEGER) === Number.MAX_SAFE_INTEGER
        ? 0
        : batches.reduce((oldest, batch) => Math.min(oldest, batch.createdAtMs || oldest), Number.MAX_SAFE_INTEGER),
      unresolvedConflicts: conflicts.length,
      lastExportAtMs: Number((await getRecord(this.db, META_STORE, 'lastExport'))?.value || 0),
      updatedAtMs: Date.now(),
    };
  }

  async export(passphrase) {
    const pendingBatches = await this.listBatches('pending');
    const conflicts = await this.listConflicts();
    const content = {
      schema: 'ctox.browser-recovery.v2',
      databaseName: this.databaseName,
      instanceId: this.instanceId,
      createdAtMs: Date.now(),
      pendingBatches,
      conflicts,
    };
    content.contentHash = await sha256Json(content);
    const encrypted = await encryptRecoveryArtifact(content, passphrase);
    const text = JSON.stringify(encrypted, null, 2);
    await putRecord(this.db, META_STORE, { key: 'lastExport', value: Date.now() });
    await this.publishStatus();
    return {
      filename: `ctox-recovery-${this.instanceId}-${new Date().toISOString().replace(/[:.]/g, '-')}.ctox-recovery`,
      blob: new Blob([text], { type: 'application/vnd.ctox.recovery+json' }),
      pendingWrites: content.pendingBatches.reduce((sum, batch) => sum + (batch.documentIds?.length || 0), 0),
    };
  }

  async previewImport(file, passphrase) {
    const text = typeof file === 'string' ? file : await file.text();
    const content = await decryptRecoveryArtifact(JSON.parse(text), passphrase);
    const expectedHash = content.contentHash;
    const hashInput = { ...content };
    delete hashInput.contentHash;
    if (!expectedHash || await sha256Json(hashInput) !== expectedHash) {
      throw recoveryError('recovery_integrity_failed', 'Recovery content hash does not match.');
    }
    if (content.instanceId !== this.instanceId || content.databaseName !== this.databaseName) {
      throw recoveryError('recovery_instance_mismatch', 'Recovery export belongs to a different CTOX instance or database.');
    }
    const previewId = globalThis.crypto?.randomUUID?.() || `preview-${Date.now()}`;
    const schemaMismatches = (content.pendingBatches || []).filter((batch) => {
      const replayer = this.replayers.get(batch.collection);
      return Boolean(batch.schemaHash && replayer?.schemaHash && batch.schemaHash !== replayer.schemaHash);
    }).map((batch) => ({
      batchId: batch.batchId,
      collection: batch.collection,
      artifactSchemaHash: batch.schemaHash,
      localSchemaHash: this.replayers.get(batch.collection)?.schemaHash || null,
    }));
    previews.set(previewId, { journal: this, content, expiresAtMs: Date.now() + 10 * 60 * 1000 });
    return {
      previewId,
      pendingBatches: content.pendingBatches?.length || 0,
      pendingWrites: (content.pendingBatches || []).reduce((sum, batch) => sum + (batch.documentIds?.length || 0), 0),
      conflicts: content.conflicts?.length || 0,
      schemaMismatches,
      createdAtMs: content.createdAtMs,
    };
  }

  async applyImport(previewId) {
    const preview = previews.get(previewId);
    if (!preview || preview.journal !== this || preview.expiresAtMs < Date.now()) {
      throw recoveryError('recovery_integrity_failed', 'Recovery import preview is missing or expired.');
    }
    previews.delete(previewId);
    for (const batch of preview.content.pendingBatches || []) {
      const existing = await getRecord(this.db, BATCH_STORE, batch.batchId);
      if (!existing) await putRecord(this.db, BATCH_STORE, { ...batch, state: 'pending' });
    }
    for (const conflict of preview.content.conflicts || []) {
      const existing = await getRecord(this.db, CONFLICT_STORE, conflict.conflictId);
      if (!existing) await putRecord(this.db, CONFLICT_STORE, { ...conflict, state: 'pending' });
    }
    const replay = await this.replayRegisteredCollections();
    await this.publishStatus();
    return { imported: true, replay };
  }

  async listBatches(state = null, collection = null) {
    const rows = (state && collection
      ? await getAllRecordsByIndex(this.db, BATCH_STORE, BATCH_STATE_COLLECTION_INDEX, [state, collection])
      : await getAllRecords(this.db, BATCH_STORE))
      .sort((left, right) => Number(left.sequence || 0) - Number(right.sequence || 0));
    return rows.filter((row) => (!state || row.state === state) && (!collection || row.collection === collection));
  }

  async gc(now = Date.now()) {
    const rows = await this.listBatches('master_acked');
    for (const row of rows) {
      if (now - Number(row.masterAckedAtMs || 0) >= ACKED_RETENTION_MS) {
        await deleteRecord(this.db, BATCH_STORE, row.batchId);
      }
    }
    await this.gcConflicts(now);
  }

  // SYNC-53: resolved conflict records hold full local+master+base documents
  // (~3x a document each) and were never reclaimed — `resolveConflict` only
  // flips state to 'resolved'. Every collision event (update_vs_update,
  // delete_vs_update, clock_skew_detected) therefore grew IndexedDB forever
  // with zero live rows. Prune conflicts that are RESOLVED and older than the
  // same 24h retention window used for master-acked batches. PENDING /
  // unresolved conflicts are user-recoverable state and are never touched
  // here; neither are unsynced WRITE batches (handled by the master_acked
  // path above and protected by §9). Returns the number of records pruned.
  async gcConflicts(now = Date.now()) {
    const rows = await getAllRecords(this.db, CONFLICT_STORE);
    let pruned = 0;
    for (const row of rows) {
      if (row.state !== 'resolved') continue;
      const resolvedAt = Number(row.resolvedAtMs || 0);
      if (!resolvedAt) {
        // A resolved record with no timestamp (legacy/imported) cannot be
        // aged safely — stamp it now so a later pass reclaims it after the
        // retention window instead of deleting a possibly-fresh resolution.
        await updateRecord(this.db, CONFLICT_STORE, row.conflictId, (current) => ({
          ...current,
          resolvedAtMs: now,
        }));
        continue;
      }
      if (now - resolvedAt >= ACKED_RETENTION_MS) {
        await deleteRecord(this.db, CONFLICT_STORE, row.conflictId);
        pruned += 1;
      }
    }
    return pruned;
  }

  async publishStatus() {
    try {
      const status = await this.getStatus();
      globalThis.dispatchEvent?.(new CustomEvent('ctox-indexeddb-recovery-status', { detail: status }));
      globalThis.localStorage?.setItem?.(
        `ctox.businessOs.recoveryStatus.${this.databaseName}`,
        JSON.stringify(status),
      );
    } catch {}
  }

  async withQuotaRecovery(operation) {
    try {
      return await operation();
    } catch (error) {
      if (!isQuotaExceeded(error) || !this.quotaCoordinator?.recover) {
        if (isQuotaExceeded(error)) throw recoveryError('indexeddb_journal_unavailable', 'Recovery journal is out of storage space.', error);
        throw error;
      }
      await this.quotaCoordinator.recover({ source: 'recovery-journal' });
      try {
        return await operation();
      } catch (retryError) {
        throw recoveryError('indexeddb_journal_unavailable', 'Recovery journal could not commit after quota recovery.', retryError);
      }
    }
  }

  close() {
    this.db.close();
  }
}

function openJournalDatabase(name) {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(name, JOURNAL_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      const batches = db.objectStoreNames.contains(BATCH_STORE)
        ? request.transaction.objectStore(BATCH_STORE)
        : db.createObjectStore(BATCH_STORE, { keyPath: 'batchId' });
      if (!batches.indexNames.contains(BATCH_STATE_COLLECTION_INDEX)) {
        batches.createIndex(BATCH_STATE_COLLECTION_INDEX, ['state', 'collection'], { unique: false });
      }
      if (!db.objectStoreNames.contains(CONFLICT_STORE)) db.createObjectStore(CONFLICT_STORE, { keyPath: 'conflictId' });
      if (!db.objectStoreNames.contains(META_STORE)) db.createObjectStore(META_STORE, { keyPath: 'key' });
    };
    request.onsuccess = () => {
      const db = request.result;
      db.onversionchange = () => db.close();
      resolve(db);
    };
    request.onerror = () => reject(request.error || new Error(`Failed to open recovery journal ${name}`));
    request.onblocked = () => reject(recoveryError('indexeddb_journal_unavailable', `Recovery journal ${name} is blocked.`));
  });
}

function transact(db, storeName, mode, run) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(storeName, mode);
    const store = tx.objectStore(storeName);
    let result;
    try {
      Promise.resolve(run(store)).then((value) => { result = value; }, (error) => {
        try { tx.abort(); } catch {}
        reject(error);
      });
    } catch (error) {
      try { tx.abort(); } catch {}
      reject(error);
    }
    tx.oncomplete = () => resolve(result);
    tx.onerror = () => reject(tx.error || new Error(`IndexedDB ${storeName} transaction failed`));
    tx.onabort = () => reject(tx.error || new Error(`IndexedDB ${storeName} transaction aborted`));
  });
}

function requestResult(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error || new Error('IndexedDB request failed'));
  });
}

function putRecord(db, storeName, value) {
  return transact(db, storeName, 'readwrite', (store) => requestResult(store.put(value)));
}

function putSequencedBatch(db, batch) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction([META_STORE, BATCH_STORE], 'readwrite');
    const meta = tx.objectStore(META_STORE);
    const batches = tx.objectStore(BATCH_STORE);
    const request = meta.get('journalSequence');
    let sequence = 0;
    request.onsuccess = () => {
      sequence = Math.max(0, Number(request.result?.value || 0)) + 1;
      batch.sequence = sequence;
      meta.put({ key: 'journalSequence', value: sequence });
      batches.put(batch);
    };
    request.onerror = () => {
      try { tx.abort(); } catch {}
      reject(request.error || new Error('Failed to allocate recovery journal sequence'));
    };
    tx.oncomplete = () => resolve(sequence);
    tx.onerror = () => reject(tx.error || new Error('Recovery journal batch transaction failed'));
    tx.onabort = () => reject(tx.error || new Error('Recovery journal batch transaction aborted'));
  });
}

function getRecord(db, storeName, key) {
  return transact(db, storeName, 'readonly', (store) => requestResult(store.get(key)));
}

function getAllRecords(db, storeName) {
  return transact(db, storeName, 'readonly', (store) => requestResult(store.getAll()));
}

function getAllRecordsByIndex(db, storeName, indexName, key) {
  return transact(db, storeName, 'readonly', (store) => requestResult(store.index(indexName).getAll(key)));
}

function deleteRecord(db, storeName, key) {
  return transact(db, storeName, 'readwrite', (store) => requestResult(store.delete(key)));
}

async function updateRecord(db, storeName, key, update) {
  return transact(db, storeName, 'readwrite', async (store) => {
    const current = await requestResult(store.get(key));
    if (!current) return false;
    await requestResult(store.put(update(current)));
    return true;
  });
}

function documentId(doc = {}, primaryPath = 'id') {
  return String(valueAtPath(doc, primaryPath) || doc.id || doc._id || doc.key || doc.uuid || '');
}

function valueAtPath(value, path) {
  return String(path || '').split('.').filter(Boolean)
    .reduce((current, segment) => current?.[segment], value);
}

// Exported for the storage layer's pull-gate overwrite journaling (SYNC-11):
// a master row that "acknowledges" the local write (same HLC, or identical
// content for mixed-version rows) is the local write's own round-trip, not
// data loss.
export function masterAcknowledgesLocal(master, local, collection = '') {
  const masterHlc = String(master?._meta?.ctoxHlc || '');
  const localHlc = String(local?._meta?.ctoxHlc || '');
  if (collection === 'business_commands' && serverAcknowledgesCommand(master, local)) return true;
  if (masterHlc && localHlc) return masterHlc === localHlc;
  return comparableDocument(master) === comparableDocument(local);
}

function serverAcknowledgesCommand(master, local) {
  const terminalOrAccepted = new Set([
    'accepted', 'completed', 'failed', 'rejected', 'cancelled', 'canceled', 'blocked',
  ]);
  if (!terminalOrAccepted.has(String(master?.status || '').toLowerCase())) return false;
  return String(master?.id || '') === String(local?.id || '')
    && String(master?.command_id || '') === String(local?.command_id || '')
    && String(master?.command_type || '') === String(local?.command_type || '')
    && String(master?.module || '') === String(local?.module || '')
    && isJsonSubset(local?.payload ?? null, master?.payload ?? null);
}

function isJsonSubset(expected, actual) {
  if (Object.is(expected, actual)) return true;
  if (Array.isArray(expected)) {
    return Array.isArray(actual)
      && expected.length === actual.length
      && expected.every((value, index) => isJsonSubset(value, actual[index]));
  }
  if (!expected || typeof expected !== 'object' || !actual || typeof actual !== 'object') return false;
  return Object.entries(expected).every(([key, value]) => (
    Object.prototype.hasOwnProperty.call(actual, key) && isJsonSubset(value, actual[key])
  ));
}

function comparableDocument(doc) {
  const copy = structuredCloneSafe(doc) || {};
  if (copy._meta) delete copy._meta.ctoxReplicationOrigin;
  return JSON.stringify(copy);
}

function normalizeConflictRows(value) {
  return Array.isArray(value) ? value : value ? [value] : [];
}

function conflictBaseById(conflict, rows) {
  if (!conflict?.base) return null;
  const baseById = {};
  for (const row of rows) {
    const id = documentId(row?.document || row, conflict.primaryPath || 'id');
    if (id) baseById[id] = structuredCloneSafe(conflict.base);
  }
  return Object.keys(baseById).length ? baseById : null;
}

function restoreAsCopy(doc) {
  const copy = structuredCloneSafe(doc) || {};
  const id = documentId(copy);
  copy.id = `${id || 'recovered'}-recovered-${Date.now().toString(36)}`;
  delete copy._rev;
  if (copy._meta) delete copy._meta.ctoxReplicationOrigin;
  return copy;
}

function estimateBytes(value) {
  try { return new TextEncoder().encode(JSON.stringify(value)).byteLength; } catch { return 0; }
}

function structuredCloneSafe(value) {
  if (value == null) return value;
  if (typeof structuredClone === 'function') return structuredClone(value);
  return JSON.parse(JSON.stringify(value));
}

function isQuotaExceeded(error) {
  return error?.name === 'QuotaExceededError' || String(error?.message || '').toLowerCase().includes('quota');
}

function recoveryError(code, message, cause = null) {
  const error = new Error(message, cause ? { cause } : undefined);
  error.code = code;
  error.retryable = code === 'indexeddb_journal_unavailable';
  return error;
}

export const recoveryJournalTestInternals = Object.freeze({
  BATCH_STORE,
  CONFLICT_STORE,
  META_STORE,
  ACKED_RETENTION_MS,
  masterAcknowledgesLocal,
});
