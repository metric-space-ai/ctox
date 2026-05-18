export async function createBusinessDb({ name }) {
  const rxdb = await loadRxdb();
  const { createRxDatabase, getRxStorageDexie } = rxdb;
  const db = await createRxDatabase({
    name,
    storage: getRxStorageDexie(),
    multiInstance: true,
    closeDuplicates: true,
  });
  return {
    mode: 'rxdb',
    raw: db,
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

function normalizeCollectionDefinition(definition) {
  if (definition?.schema) return definition;
  return { schema: definition };
}

async function loadRxdb() {
  const mod = await import('../vendor/rxdb-bundle.mjs');
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
