export function createMockBusinessOsContext({
  host = document.getElementById('app') || document.body,
  collections = {},
  preferences = {},
} = {}) {
  const stores = new Map();
  for (const [name, records] of Object.entries(collections)) {
    stores.set(name, Array.isArray(records) ? [...records] : []);
  }
  const commandLog = [];

  return {
    host,
    preferences: {
      theme: document.documentElement.dataset.theme === 'light' ? 'light' : 'dark',
      branding: null,
      ...preferences,
    },
    db: {
      collection(name) {
        const key = String(name || '').trim();
        if (!key) throw new Error('collection name is required');
        if (!stores.has(key)) stores.set(key, []);
        return createMockCollection(stores.get(key));
      },
    },
    commandBus: {
      async dispatch(command) {
        const id = command?.id || command?.command_id || `cmd_${Date.now()}`;
        const entry = {
          ...command,
          id,
          command_id: id,
          status: 'completed',
          updated_at_ms: Date.now(),
        };
        commandLog.push(entry);
        return entry;
      },
      log: commandLog,
    },
  };
}

function createMockCollection(records) {
  return {
    async insert(record) {
      const next = { ...record, id: record?.id || `rec_${Date.now()}` };
      records.push(next);
      return createMockDoc(next);
    },
    find() {
      return {
        async exec() {
          return records.map(createMockDoc);
        },
      };
    },
    findOne(id) {
      return {
        async exec() {
          const found = records.find((record) => record.id === id);
          return found ? createMockDoc(found) : null;
        },
      };
    },
  };
}

function createMockDoc(record) {
  return {
    toJSON() {
      return { ...record };
    },
    async patch(update) {
      Object.assign(record, update);
      return createMockDoc(record);
    },
    async remove() {
      record._deleted = true;
      record.is_deleted = true;
      return createMockDoc(record);
    },
  };
}
