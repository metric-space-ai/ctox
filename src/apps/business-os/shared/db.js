export async function createBusinessDb({ name }) {
  try {
    return await createRxBusinessDb({ name });
  } catch (error) {
    console.error('[Fallback DB] Failed to create RxDB database, falling back to local storage database:', error);
    return new FallbackDatabase();
  }
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

class FallbackCollection {
  constructor(name) {
    this.name = name;
    this.storageKey = `ctox.fallbackDb.${name}`;
    this.subscribers = new Set();
  }

  _getData() {
    try {
      return JSON.parse(localStorage.getItem(this.storageKey) || '{}');
    } catch {
      return {};
    }
  }

  _setData(data) {
    try {
      localStorage.setItem(this.storageKey, JSON.stringify(data));
    } catch (e) {
      console.warn('Fallback DB storage failed', e);
    }
  }

  _triggerSubscribers() {
    for (const callback of this.subscribers) {
      try {
        callback();
      } catch (err) {
        console.error('Subscriber error:', err);
      }
    }
  }

  _makeDocument(item) {
    if (!item) return null;
    return {
      ...item,
      toJSON: () => ({ ...item }),
      incrementalPatch: async (patch) => {
        const currentData = this._getData();
        const currentItem = currentData[item.id] || {};
        const nextItem = { ...currentItem, ...patch };
        currentData[item.id] = nextItem;
        this._setData(currentData);
        this._triggerSubscribers();
        return this._makeDocument(nextItem);
      },
      remove: async () => {
        const currentData = this._getData();
        delete currentData[item.id];
        this._setData(currentData);
        this._triggerSubscribers();
      }
    };
  }

  find() {
    const queryObj = {
      exec: async () => {
        const data = this._getData();
        return Object.values(data).map(item => this._makeDocument(item));
      },
      $: {
        subscribe: (callback) => {
          const runEmit = async () => {
            const docs = await queryObj.exec();
            callback(docs);
          };
          runEmit();

          const listener = async () => {
            const docs = await queryObj.exec();
            callback(docs);
          };
          this.subscribers.add(listener);

          return {
            unsubscribe: () => {
              this.subscribers.delete(listener);
            }
          };
        }
      }
    };
    return queryObj;
  }

  findOne(id) {
    return {
      exec: async () => {
        const data = this._getData();
        const item = data[id];
        if (!item) return null;
        return this._makeDocument(item);
      }
    };
  }

  async insert(payload) {
    const id = payload.id || 'default';
    const data = this._getData();
    data[id] = { ...payload };
    this._setData(data);
    this._triggerSubscribers();
    return this._makeDocument(payload);
  }
}

class FallbackDatabase {
  constructor() {
    this.mode = 'fallback';
    this.collectionsMap = {};
    this.raw = new Proxy(this.collectionsMap, {
      get: (target, name) => {
        if (typeof name === 'symbol') return target[name];
        return this.collection(name);
      }
    });
  }

  get collections() {
    return new Proxy(this.collectionsMap, {
      get: (target, name) => {
        if (typeof name === 'symbol') return target[name];
        return this.collection(name);
      }
    });
  }

  async addCollections(collections) {
    if (!collections) return;
    for (const name of Object.keys(collections)) {
      if (!this.collectionsMap[name]) {
        this.collectionsMap[name] = new FallbackCollection(name);
      }
    }
  }

  collection(name) {
    if (!this.collectionsMap[name]) {
      this.collectionsMap[name] = new FallbackCollection(name);
    }
    return this.collectionsMap[name];
  }

  close() {
    // noop
  }
}
