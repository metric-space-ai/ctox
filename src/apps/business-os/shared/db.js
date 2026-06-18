const CTOX_RXDB_RUNTIME = Object.freeze({
  name: 'ctox-rxdb-js',
  publicName: 'CTOX DB',
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
const RXDB_RECOVERY_OPEN_TIMEOUT_MS = 20000;
const RXDB_BUNDLE_URL = '../rxdb/dist/ctox-rxdb-js.mjs?v=20260618-ats-collections';

export async function createBusinessDb({ name }) {
  try {
    const db = await Promise.race([
      createRxBusinessDb({ name }),
      timeoutAfter(RXDB_OPEN_TIMEOUT_MS, `RxDB database creation timed out after ${RXDB_OPEN_TIMEOUT_MS}ms (possible IndexedDB lock)`),
    ]);
    clearRecoveryDatabaseName(name);
    return db;
  } catch (error) {
    if (!isIndexedDbOpenStall(error)) throw error;
    console.info('[business-os] IndexedDB open stalled; retrying local RxDB open once without deleting cache', error);
    try {
      const db = await Promise.race([
        createRxBusinessDb({ name }),
        timeoutAfter(RXDB_OPEN_RETRY_TIMEOUT_MS, `RxDB database retry timed out after ${RXDB_OPEN_RETRY_TIMEOUT_MS}ms (possible IndexedDB lock)`),
      ]);
      clearRecoveryDatabaseName(name);
      return db;
    } catch (retryError) {
      if (!isIndexedDbOpenStall(retryError)) throw retryError;
      return openRecoveryBusinessDb({ name, cause: retryError });
    }
  }
}

export async function resetBusinessDb({ name }) {
  await Promise.race([
    deleteIndexedDb(name),
    timeoutAfter(RXDB_RESET_TIMEOUT_MS, `RxDB database reset timed out after ${RXDB_RESET_TIMEOUT_MS}ms (possible IndexedDB lock)`),
  ]);
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
    close: () => db.close(),
  };
}

async function openRecoveryBusinessDb({ name, cause }) {
  const recoveryName = recoveryDatabaseName(name);
  console.warn('[business-os] primary IndexedDB remained blocked; opening recovery database', {
    name,
    recoveryName,
    error: String(cause?.message || cause || ''),
  });
  const db = await Promise.race([
    createRxBusinessDb({ name: recoveryName }),
    timeoutAfter(RXDB_RECOVERY_OPEN_TIMEOUT_MS, `RxDB recovery database open timed out after ${RXDB_RECOVERY_OPEN_TIMEOUT_MS}ms (primary IndexedDB remained blocked)`),
  ]);
  return {
    ...db,
    recovery: {
      requestedName: name,
      activeName: recoveryName,
      reason: String(cause?.message || cause || ''),
    },
  };
}

function recoveryDatabaseName(name) {
  const key = `ctox.businessOs.rxdbRecoveryDb.${name}`;
  try {
    const existing = sessionStorage.getItem(key);
    if (existing) return existing;
    const next = `${name}__recovery_${Date.now().toString(36)}`;
    sessionStorage.setItem(key, next);
    return next;
  } catch {
    return `${name}__recovery_${Date.now().toString(36)}`;
  }
}

function clearRecoveryDatabaseName(name) {
  try {
    sessionStorage.removeItem(`ctox.businessOs.rxdbRecoveryDb.${name}`);
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
    request.onblocked = () => resolve();
    request.onsuccess = () => {
      const db = request.result;
      db.close();
      const deleteRequest = indexedDb.deleteDatabase(probeName);
      deleteRequest.onsuccess = () => resolve();
      deleteRequest.onerror = () => reject(deleteRequest.error || new Error(`Failed to delete IndexedDB probe ${probeName}`));
      deleteRequest.onblocked = () => resolve();
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
    request.onblocked = () => resolve();
  });
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
