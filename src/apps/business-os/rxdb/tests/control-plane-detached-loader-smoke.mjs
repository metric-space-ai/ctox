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
  collection.setDemandLoader({
    currentReadPermissionDigest: () => 'authorized-epoch',
    resolveQuery: async () => [rows.get(name)],
  });
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
  // No storage event accompanies a WebRTC loader detach. Existing subscribers
  // must still lose their last authorized row immediately.
  await waitFor(() => snapshots.length >= 2 && querySnapshots.length >= 2);
  assert(snapshots[1].documents.length === 0, `${name}: collection subscription leaked a stale row`);
  assert(querySnapshots[1].length === 0, `${name}: query subscription leaked a stale row`);
  for (const listener of listeners.get(name)) listener({});
  await new Promise((resolve) => setTimeout(resolve, 80));
  assert(snapshots.every((value, index) => index === 0 || value.documents.length === 0), `${name}: later storage event restored a stale row`);
  assert(querySnapshots.every((value, index) => index === 0 || value.length === 0), `${name}: later query event restored a stale row`);
  collectionSubscription.unsubscribe();
  querySubscription.unsubscribe();
}

// A fetch started under an earlier bridge may complete after detach. Its
// result cannot become the initial value of either live subscription.
const delayedCollection = database.business_commands;
let releaseOldFetch;
const oldFetch = new Promise((resolve) => { releaseOldFetch = resolve; });
delayedCollection.setDemandLoader({ resolveQuery: () => oldFetch });
const delayedSnapshots = [];
const delayedQuerySnapshots = [];
const delayedSubscription = delayedCollection.$.subscribe((value) => delayedSnapshots.push(value));
const delayedQuerySubscription = delayedCollection.find().$.subscribe((value) => delayedQuerySnapshots.push(value));
delayedCollection.setDemandLoader(null);
releaseOldFetch([rows.get('business_commands')]);
await waitFor(() => delayedSnapshots.length > 0 && delayedQuerySnapshots.length > 0);
await new Promise((resolve) => setTimeout(resolve, 20));
assert(delayedSnapshots.every((value) => value.documents.length === 0), 'late collection fetch published superseded row');
assert(delayedQuerySnapshots.every((value) => value.length === 0), 'late query fetch published superseded row');
delayedSubscription.unsubscribe();
delayedQuerySubscription.unsubscribe();

const manyRows = Array.from({ length: 205 }, (_, index) => ({ id: `command-${index}`, state: 'complete' }));
delayedCollection.setDemandLoader({
  currentReadPermissionDigest: () => 'authorized-epoch',
  resolveQuery: async (query) => manyRows.slice(query.skip || 0, (query.skip || 0) + query.limit),
});
assert(await delayedCollection.count().exec() === 205, 'authorized count stopped at the first 200-row window');
assert(await delayedCollection.count({ skip: 5, limit: 7 }).exec() === 7, 'authorized count lost skip/limit semantics');
let digest = 'authorized-epoch';
let pageCalls = 0;
delayedCollection.setDemandLoader({
  currentReadPermissionDigest: () => digest,
  resolveQuery: async (query) => {
    pageCalls += 1;
    if (pageCalls === 2) {
      digest = '';
      return [];
    }
    return manyRows.slice(query.skip || 0, (query.skip || 0) + query.limit);
  },
});
assert(await delayedCollection.count().exec() === 0, 'mid-count identity loss leaked an earlier page count');
assert(pageCalls === 2, 'mid-count fixture did not cross the second demand window');
delayedCollection.setDemandLoader(null);
assert(await delayedCollection.count().exec() === 0, 'detached loader leaked the multi-window count');

assert(directReads === 0, 'control-plane reads must never use raw storage fallback');
assert((await database.ordinary_records.find().exec())[0]?.id === 'record-1', 'ordinary collection local read changed');
assert(directReads === 1, 'ordinary collection must retain direct local query');
await database.close();
console.log('control-plane detached-loader smoke: ok');
