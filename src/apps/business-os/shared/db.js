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
      for (const [collectionName, schema] of Object.entries(collections)) {
        if (!db[collectionName]) {
          missing[collectionName] = { schema };
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

async function loadRxdb() {
  const mod = await import('../vendor/rxdb-bundle.mjs');
  if (typeof mod.rxdbCore === 'function') return mod.rxdbCore();
  if (globalThis.rxdbCore) return globalThis.rxdbCore();
  return mod;
}
