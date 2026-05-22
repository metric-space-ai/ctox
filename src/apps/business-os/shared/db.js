export async function createBusinessDb({ name }) {
  const timeoutPromise = new Promise((_, reject) => {
    setTimeout(() => reject(new Error('RxDB database creation timed out after 25000ms (possible IndexedDB lock)')), 25000);
  });
  return Promise.race([
    createRxBusinessDb({ name }),
    timeoutPromise,
  ]);
}

export async function resetBusinessDb({ name }) {
  const rxdb = await loadRxdb();
  const storage = rxdb.getRxStorageDexie();
  if (typeof rxdb.removeRxDatabase === 'function') {
    await rxdb.removeRxDatabase(name, storage, false);
    return;
  }
  await deleteIndexedDb(name);
}

async function createRxBusinessDb({ name }) {
  const rxdb = await loadRxdb();
  const { createRxDatabase, getRxStorageDexie } = rxdb;
  const db = await createRxDatabase({
    name,
    storage: getRxStorageDexie(),
    multiInstance: false,
    closeDuplicates: true,
  });
  return {
    mode: 'rxdb',
    rxdb,
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
  const mod = await import('../vendor/rxdb-bundle.mjs?v=20260522-rxdb-fork1');
  registerRxdbPlugin(mod, mod.RxDBMigrationSchemaPlugin);
  const rxdb = typeof mod.rxdbCore === 'function'
    ? mod.rxdbCore()
    : (globalThis.rxdbCore || mod);
  registerRxdbPlugin(rxdb, rxdb.RxDBMigrationSchemaPlugin || mod.RxDBMigrationSchemaPlugin || mod.RxDBMigrationPlugin);
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
