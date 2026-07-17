const CTOX_RXDB_RUNTIME = Object.freeze({
  name: 'ctox-rxdb-js',
  publicName: 'CTOX Sync Engine',
  source: 'app-local',
  importPath: 'src/apps/business-os/rxdb/dist/ctox-rxdb-js.mjs',
  packageManager: 'none',
  compatibility: 'ctox-db-api',
  upstreamCompatible: false,
  upstreamCompatibility: 'not-upstream-rxdb',
  apiContract: 'ctox-db-business-os-v1',
});

const RXDB_OPEN_TIMEOUT_MS = 45000;
const RXDB_OPEN_RETRY_TIMEOUT_MS = 12000;
const RXDB_RESET_TIMEOUT_MS = 5000;
const INDEXEDDB_PREFLIGHT_TIMEOUT_MS = 8000;
const RXDB_MODULE_IMPORT_TIMEOUT_MS = 8000;
const RXDB_CREATE_DATABASE_TIMEOUT_MS = 8000;
const RXDB_BUNDLE_URL = '../rxdb/dist/ctox-rxdb-js.mjs?v=20260715-office-file-stream-v65';

export async function createBusinessDb({ name }) {
  const storageHealth = await inspectBrowserStorageDurability(name);
  try {
    const db = await Promise.race([
      createRxBusinessDb({ name }),
      timeoutAfter(RXDB_OPEN_TIMEOUT_MS, `RxDB database creation timed out after ${RXDB_OPEN_TIMEOUT_MS}ms (possible IndexedDB lock)`),
    ]);
    clearRecoveryDatabaseName(name);
    return attachDatabaseDurability(db, storageHealth);
  } catch (error) {
    if (!isIndexedDbOpenStall(error)) throw error;
    console.info('[business-os] IndexedDB open stalled; retrying local RxDB open once without deleting cache', error);
    try {
      const db = await Promise.race([
        createRxBusinessDb({ name }),
        timeoutAfter(RXDB_OPEN_RETRY_TIMEOUT_MS, `RxDB database retry timed out after ${RXDB_OPEN_RETRY_TIMEOUT_MS}ms (possible IndexedDB lock)`),
      ]);
      clearRecoveryDatabaseName(name);
      return attachDatabaseDurability(db, storageHealth);
    } catch (retryError) {
      if (!isIndexedDbOpenStall(retryError)) throw retryError;
      const journal = recordBlockedRecoveryJournal(name, retryError, storageHealth);
      const blocked = new Error(`Primary IndexedDB remains blocked; writes are disabled until the stale handle closes (${name}).`);
      blocked.code = 'indexeddb_blocked';
      blocked.recovery = journal;
      throw blocked;
    }
  }
}

export async function resetBusinessDb({ name }) {
  const recoveryStatus = readRecoveryStatus(name);
  if (
    Number(recoveryStatus?.pendingWrites || 0) > 0
    && Number(recoveryStatus?.lastExportAtMs || 0) < Number(recoveryStatus?.oldestPendingAtMs || 0)
  ) {
    const error = new Error(`Recovery export required before resetting IndexedDB ${name}.`);
    error.code = 'recovery_export_required';
    error.recovery = recoveryStatus;
    throw error;
  }
  await Promise.race([
    deleteIndexedDb(name),
    timeoutAfter(RXDB_RESET_TIMEOUT_MS, `RxDB database reset timed out after ${RXDB_RESET_TIMEOUT_MS}ms (possible IndexedDB lock)`),
  ]);
}

function readRecoveryStatus(name) {
  try {
    return JSON.parse(localStorage.getItem(`ctox.businessOs.recoveryStatus.${name}`) || 'null');
  } catch {
    return null;
  }
}

function timeoutAfter(ms, message) {
  return new Promise((_, reject) => {
    setTimeout(() => reject(new Error(message)), ms);
  });
}

function isIndexedDbOpenStall(error) {
  const message = String(error?.message || error || '').toLowerCase();
  return message.includes('indexeddb open timed out')
    || message.includes('indexeddb open blocked')
    || message.includes('rxdb database creation timed out')
    || message.includes('rxdb database retry timed out')
    || message.includes('rxdb createdatabase timed out')
    || message.includes('rxdb bundle import timed out')
    || message.includes('indexeddb lock');
}

async function createRxBusinessDb({ name }) {
  await prepareIndexedDbForRxdb(name);
  const rxdb = await loadRxdb();
  const { createRxDatabase, getCtoxIndexedDbStorage } = rxdb;
  const db = await Promise.race([
    createRxDatabase({
      name,
      storage: getCtoxIndexedDbStorage(),
      multiInstance: false,
      closeDuplicates: true,
    }),
    timeoutAfter(RXDB_CREATE_DATABASE_TIMEOUT_MS, `RxDB createRxDatabase timed out after ${RXDB_CREATE_DATABASE_TIMEOUT_MS}ms (possible IndexedDB lock)`),
  ]);
  return {
    mode: 'rxdb',
    name,
    rxdb,
    runtime: rxdb.__ctoxRuntime || CTOX_RXDB_RUNTIME,
    raw: db,
    get collections() {
      return db.collections;
    },
    async addCollections(collections) {
      if (!collections || !Object.keys(collections).length) return;
      const missing = {};
      for (const [collectionName, definition] of Object.entries(collections)) {
        if (!db[collectionName]) {
          missing[collectionName] = normalizeCollectionDefinition(definition);
        }
      }
      if (Object.keys(missing).length) {
        await db.addCollections(missing);
      }
    },
    collection: (name) => db[name],
    getUnsyncedWriteSummary: () => db.getUnsyncedWriteSummary?.() || Promise.resolve({ total: 0, byCollection: {} }),
    recovery: db.recovery,
    conflicts: db.conflicts,
    close: () => db.close(),
  };
}

function clearRecoveryDatabaseName(name) {
  try {
    const key = `ctox.businessOs.rxdbRecoveryJournal.${name}`;
    const current = JSON.parse(localStorage.getItem(key) || 'null');
    if (!current || Number(current.uniqueUnsyncedWrites || 0) === 0) {
      localStorage.removeItem(key);
    }
  } catch {}
}

async function prepareIndexedDbForRxdb(name) {
  const indexedDb = globalThis.indexedDB;
  if (!indexedDb?.open) return;
  const probeName = `${name}__ctox_probe`;
  try {
    await Promise.race([
      openAndDeleteProbeDatabase(indexedDb, probeName),
      timeoutAfter(INDEXEDDB_PREFLIGHT_TIMEOUT_MS, `IndexedDB preflight timed out after ${INDEXEDDB_PREFLIGHT_TIMEOUT_MS}ms`),
    ]);
  } catch (error) {
    if (!isIndexedDbPreflightTimeout(error)) throw error;
    console.info('[business-os] IndexedDB preflight timed out; continuing with guarded RxDB open', error);
  }
}

function isIndexedDbPreflightTimeout(error) {
  return String(error?.message || error || '').includes('IndexedDB preflight timed out');
}

function openAndDeleteProbeDatabase(indexedDb, probeName) {
  return new Promise((resolve, reject) => {
    const request = indexedDb.open(probeName, 1);
    request.onerror = () => reject(request.error || new Error(`Failed to open IndexedDB probe ${probeName}`));
    request.onblocked = () => reject(new Error(`IndexedDB probe open blocked for ${probeName}`));
    request.onsuccess = () => {
      const db = request.result;
      db.close();
      const deleteRequest = indexedDb.deleteDatabase(probeName);
      deleteRequest.onsuccess = () => resolve();
      deleteRequest.onerror = () => reject(deleteRequest.error || new Error(`Failed to delete IndexedDB probe ${probeName}`));
      deleteRequest.onblocked = () => reject(new Error(`IndexedDB probe delete blocked for ${probeName}`));
    };
  });
}

function deleteIndexedDb(name) {
  return new Promise((resolve, reject) => {
    const indexedDb = globalThis.indexedDB;
    if (!indexedDb?.deleteDatabase) {
      resolve();
      return;
    }
    const request = indexedDb.deleteDatabase(name);
    request.onsuccess = () => resolve();
    request.onerror = () => reject(request.error || new Error(`Failed to delete IndexedDB ${name}`));
    request.onblocked = () => reject(new Error(`IndexedDB delete blocked for ${name}`));
  });
}

async function inspectBrowserStorageDurability(databaseName = '') {
  const storage = globalThis.navigator?.storage;
  let persistent = null;
  let persistenceRequested = false;
  let estimate = null;
  try {
    persistent = typeof storage?.persisted === 'function' ? await storage.persisted() : null;
    if (persistent === false && typeof storage?.persist === 'function') {
      persistenceRequested = true;
      persistent = await storage.persist();
    }
  } catch {}
  try {
    estimate = typeof storage?.estimate === 'function' ? await storage.estimate() : null;
  } catch {}
  const usage = Number(estimate?.usage || 0);
  const quota = Number(estimate?.quota || 0);
  let unsyncedWrites = null;
  let recoveryStatus = null;
  try {
    unsyncedWrites = JSON.parse(
      localStorage.getItem(`ctox.businessOs.unsyncedWrites.${databaseName}`) || 'null',
    );
  } catch {}
  try {
    recoveryStatus = JSON.parse(
      localStorage.getItem(`ctox.businessOs.recoveryStatus.${databaseName}`) || 'null',
    );
  } catch {}
  return {
    persistent,
    persistenceRequested,
    usageBytes: usage,
    quotaBytes: quota,
    pressureRatio: quota > 0 ? usage / quota : null,
    ephemeralLikely: quota > 0 && quota < 128 * 1024 * 1024,
    wasDiscarded: Boolean(globalThis.document?.wasDiscarded),
    unsyncedWrites: Number(unsyncedWrites?.total || 0),
    unsyncedByCollection: unsyncedWrites?.byCollection || {},
    journalPendingWrites: Number(recoveryStatus?.pendingWrites || 0),
    journalPendingBytes: Number(recoveryStatus?.pendingBytes || 0),
    unresolvedConflicts: Number(recoveryStatus?.unresolvedConflicts || 0),
    lastRecoveryExportAtMs: Number(recoveryStatus?.lastExportAtMs || 0),
    capturedAtMs: Date.now(),
  };
}

function recordBlockedRecoveryJournal(name, cause, storageHealth) {
  let persistedUnsynced = null;
  try {
    persistedUnsynced = JSON.parse(
      localStorage.getItem(`ctox.businessOs.unsyncedWrites.${name}`) || 'null',
    );
  } catch {}
  const journal = {
    schema: 'ctox.indexeddb.recovery-journal.v1',
    databaseName: name,
    state: 'blocked',
    uniqueUnsyncedWrites: Number(persistedUnsynced?.total || 0),
    unsyncedByCollection: persistedUnsynced?.byCollection || {},
    reason: String(cause?.message || cause || ''),
    storageHealth,
    updatedAtMs: Date.now(),
  };
  try {
    localStorage.setItem(`ctox.businessOs.rxdbRecoveryJournal.${name}`, JSON.stringify(journal));
  } catch {}
  return journal;
}

function attachDatabaseDurability(db, storageHealth) {
  const lifecycle = { state: 'active', lastEvent: 'open', updatedAtMs: Date.now() };
  const update = (state, event) => {
    lifecycle.state = state;
    lifecycle.lastEvent = event;
    lifecycle.updatedAtMs = Date.now();
    globalThis.dispatchEvent?.(new CustomEvent('ctox-rxdb-lifecycle', {
      detail: { databaseName: db.name, state, event, updatedAtMs: lifecycle.updatedAtMs },
    }));
    if (
      ['freeze', 'pagehide'].includes(event)
      && Number(storageHealth.journalPendingWrites || storageHealth.unsyncedWrites || 0) > 0
      && (
        storageHealth.persistent === false
        || storageHealth.ephemeralLikely
        || Number(storageHealth.pressureRatio || 0) >= 0.8
      )
    ) {
      globalThis.dispatchEvent?.(new CustomEvent('ctox-indexeddb-recovery-required', {
        detail: { databaseName: db.name, event, ...storageHealth },
      }));
    }
  };
  const listeners = [
    [globalThis.document, 'visibilitychange', () => update(document.visibilityState === 'hidden' ? 'background' : 'active', 'visibilitychange')],
    [globalThis.document, 'freeze', () => update('frozen', 'freeze')],
    [globalThis.document, 'resume', () => update('active', 'resume')],
    [globalThis, 'pagehide', () => update('pagehide', 'pagehide')],
    [globalThis, 'pageshow', () => update('active', 'pageshow')],
  ];
  for (const [target, type, handler] of listeners) target?.addEventListener?.(type, handler);
  const refreshStorageHealth = async () => {
    const latest = await inspectBrowserStorageDurability(db.name);
    Object.assign(storageHealth, latest);
    if (Number(latest.pressureRatio || 0) >= 0.8) {
      globalThis.dispatchEvent?.(new CustomEvent('ctox-indexeddb-storage-pressure', {
        detail: { databaseName: db.name, ...latest },
      }));
    }
    return storageHealth;
  };
  const storageHealthTimer = setInterval(() => {
    refreshStorageHealth().catch(() => {});
  }, 60_000);
  const recoveryStatusListener = (event) => {
    if (event?.detail?.databaseName !== db.name) return;
    Object.assign(storageHealth, {
      journalPendingWrites: Number(event.detail.pendingWrites || 0),
      journalPendingBytes: Number(event.detail.pendingBytes || 0),
      oldestPendingAtMs: Number(event.detail.oldestPendingAtMs || 0),
      unresolvedConflicts: Number(event.detail.unresolvedConflicts || 0),
      lastRecoveryExportAtMs: Number(event.detail.lastExportAtMs || 0),
    });
  };
  globalThis.addEventListener?.('ctox-indexeddb-recovery-status', recoveryStatusListener);
  storageHealthTimer.unref?.();
  const close = db.close;
  db.close = async () => {
    clearInterval(storageHealthTimer);
    globalThis.removeEventListener?.('ctox-indexeddb-recovery-status', recoveryStatusListener);
    for (const [target, type, handler] of listeners) target?.removeEventListener?.(type, handler);
    update('closed', 'close');
    return close();
  };
  db.storageHealth = storageHealth;
  db.refreshStorageHealth = refreshStorageHealth;
  db.lifecycle = lifecycle;
  return db;
}

function normalizeCollectionDefinition(definition) {
  if (definition?.schema) return definition;
  return { schema: definition };
}

async function loadRxdb() {
  let mod = null;
  try {
    mod = await importRxdbBundle(RXDB_BUNDLE_URL);
  } catch (error) {
    if (!isIndexedDbOpenStall(error)) throw error;
    console.info('[business-os] RxDB bundle import stalled; retrying with cache-busted module graph', error);
    mod = await importRxdbBundle(`${RXDB_BUNDLE_URL}&retry=${Date.now().toString(36)}`);
  }
  return materializeRxdbRuntime(mod);
}

async function importRxdbBundle(url) {
  return Promise.race([
    import(url),
    timeoutAfter(RXDB_MODULE_IMPORT_TIMEOUT_MS, `RxDB bundle import timed out after ${RXDB_MODULE_IMPORT_TIMEOUT_MS}ms`),
  ]);
}

function materializeRxdbRuntime(mod) {
  registerRxdbPlugin(mod, mod.RxDBMigrationSchemaPlugin);
  const rxdb = typeof mod.rxdbCore === 'function'
    ? mod.rxdbCore()
    : (globalThis.rxdbCore || mod);
  registerRxdbPlugin(rxdb, rxdb.RxDBMigrationSchemaPlugin || mod.RxDBMigrationSchemaPlugin || mod.RxDBMigrationPlugin);
  if (!rxdb.__ctoxRuntime) {
    Object.defineProperty(rxdb, '__ctoxRuntime', {
      value: CTOX_RXDB_RUNTIME,
      enumerable: true,
    });
  }
  return rxdb;
}

function registerRxdbPlugin(target, plugin) {
  const add = target?.addRxPlugin;
  if (typeof add !== 'function' || !plugin) return;
  try {
    add(plugin);
  } catch (error) {
    const message = String(error?.message || error || '');
    if (!message.toLowerCase().includes('already')) throw error;
  }
}
