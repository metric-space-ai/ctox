const { createRxDatabase } = await import(
  process.argv.includes('--source') ? '../src/rx-database.mjs' : '../dist/ctox-rxdb-js.mjs'
);

const rows = new Map([
  ['business_commands', { id: 'command-1', state: 'complete' }],
  ['ctox_queue_tasks', { id: 'task-1', state: 'complete' }],
  ['ordinary_records', { id: 'record-1', state: 'visible' }],
]);
const listeners = new Map();
let directReads = 0;
const database = await createRxDatabase({
  name: 'control-plane-detached-loader',
  storage: {
    nativeStorage: {
      collection(name) {
        const listenerSet = new Set();
        listeners.set(name, listenerSet);
        return {
          observe(listener) {
            listenerSet.add(listener);
            return () => listenerSet.delete(listener);
          },
          async queryDocuments() {
            directReads += 1;
            return [rows.get(name)];
          },
          async countDocuments() {
            directReads += 1;
            return 1;
          },
        };
      },
      close() {},
    },
  },
});
const schema = {
  version: 0,
  primaryKey: 'id',
  type: 'object',
  properties: { id: { type: 'string' }, state: { type: 'string' } },
};
await database.addCollections(Object.fromEntries(
  rows.keys().map((name) => [name, { schema }]),
));

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};
const waitFor = async (predicate) => {
  const deadline = Date.now() + 1000;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error('timed out waiting for guarded subscription');
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
};

for (const name of ['business_commands', 'ctox_queue_tasks']) {
  const collection = database.collection(name);
  collection.setDemandLoader({ resolveQuery: async () => [rows.get(name)] });
  assert((await collection.find().exec()).length === 1, `${name}: authorized loader must supply the row`);
  assert(await collection.count().exec() === 1, `${name}: count must use the authorized window`);

  const snapshots = [];
  const querySnapshots = [];
  const collectionSubscription = collection.$.subscribe((value) => snapshots.push(value));
  const querySubscription = collection.find().$.subscribe((value) => querySnapshots.push(value));
  await waitFor(() => snapshots.length === 1 && querySnapshots.length === 1);

  // Replication cancellation removes the demand loader while the populated
  // local store remains. Neither a fresh query nor an existing subscription
  // may publish that stale row under an unknown/revoked read identity.
  collection.setDemandLoader(null);
  assert((await collection.find().exec()).length === 0, `${name}: detached loader leaked find()`);
  assert(await collection.findOne(rows.get(name).id).exec() === null, `${name}: detached loader leaked findOne()`);
  assert(await collection.count().exec() === 0, `${name}: detached loader leaked count()`);
  for (const listener of listeners.get(name)) listener({});
  await waitFor(() => snapshots.length === 2 && querySnapshots.length === 2);
  assert(snapshots[1].documents.length === 0, `${name}: collection subscription leaked a stale row`);
  assert(querySnapshots[1].length === 0, `${name}: query subscription leaked a stale row`);
  collectionSubscription.unsubscribe();
  querySubscription.unsubscribe();
}

assert(directReads === 0, 'control-plane reads must never use raw storage fallback');
assert((await database.ordinary_records.find().exec())[0]?.id === 'record-1', 'ordinary collection local read changed');
assert(directReads === 1, 'ordinary collection must retain direct local query');
await database.close();
console.log('control-plane detached-loader smoke: ok');
