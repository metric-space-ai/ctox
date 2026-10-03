import assert from 'node:assert/strict';
const { createRxDatabase } = await import(
  process.argv.includes('--source') ? '../src/index.mjs' : '../dist/ctox-rxdb-js.mjs'
);
const rows = Array.from({ length: 871 }, (_, index) => ({ id: `chat-${String(index).padStart(4, '0')}`, version: 0 }));
const observers = new Set();
let localReads = 0;
const db = await createRxDatabase({ name: 'live-query-window-bound', storage: { nativeStorage: {
  collection: () => ({
    observe(listener) { observers.add(listener); return () => observers.delete(listener); },
    async queryDocuments(query) {
      localReads += 1;
      return rows.slice(0, query.limit ?? rows.length);
    },
  }), close() {},
} } });
await db.addCollections({ business_chats: { schema: { version: 0, primaryKey: 'id', type: 'object',
  properties: { id: { type: 'string' }, version: { type: 'number' } } } } });
const chats = db.business_chats;
chats.setDemandLoader({ resolveQuery() { throw new Error('complete replica must read locally'); } });
chats.setLocalReplicaComplete(true);
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(predicate) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (predicate()) return;
    await pause(5);
  }
  assert.ok(predicate(), 'bounded wait expired');
}
const emit = success => { for (const observer of observers) observer({ success }); };
let subscription;
try {
  const emissions = [];
  subscription = chats.find().$.subscribe(value => emissions.push(value));
  await waitFor(() => emissions.length === 1);
  assert.equal(emissions[0].length, 200);
  // A real eager pull reports all changed records, not just the visible query
  // window. Applying its raw deltas must not grow that window to the whole DB.
  emit(Object.fromEntries(rows.map(row => [row.id, { ...row, version: 1 }])));
  await waitFor(() => emissions.length === 2);
  assert.equal(emissions[1].length, 200, 'live changes must preserve the implicit 200-row window');
  assert.equal(localReads, 2, 'windowed live selection must requery its actual boundary');
  subscription.unsubscribe();

  chats.setLocalReplicaComplete(false);
  let authorityReads = 0;
  chats.setDemandLoader({ async resolveQuery(query) {
    assert.equal(query.requireRevision, 'authority');
    authorityReads += 1;
    return [{ id: 'authorized', version: authorityReads }];
  } });
  const strict = [];
  subscription = chats.find({ selector: {}, requireRevision: 'authority' }).$.subscribe(value => strict.push(value));
  await waitFor(() => strict.length === 1);
  emit({ unauthorized: { id: 'unauthorized', version: 99 } });
  await waitFor(() => strict.length === 2);
  assert.deepEqual(strict.at(-1).map(row => row.id), ['authorized'], 'storage delta has no native window permission stamp');
  assert.equal(authorityReads, 2);
  subscription.unsubscribe();

  const primary = [];
  subscription = chats.findOne({ selector: { id: 'authorized' }, requireRevision: 'authority' }).$.subscribe(value => primary.push(value));
  await waitFor(() => primary.length === 1);
  emit({ authorized: { id: 'authorized', version: 99 } });
  await waitFor(() => primary.length === 2);
  assert.equal(primary.at(-1).version, 4, 'even a matching primary-key delta cannot replace strict authority');
  subscription.unsubscribe();

  chats.setDemandLoader(null);
  const attaching = [];
  subscription = chats.find().$.subscribe(value => attaching.push(value));
  await waitFor(() => attaching.length === 1);
  assert.equal(attaching[0].length, 871, 'genuinely unbounded local queries retain their contract');
  chats.setDemandLoader({ resolveQuery() { throw new Error('complete replica must read locally'); } });
  chats.setLocalReplicaComplete(true);
  emit({ added: { id: 'added', version: 1 } });
  await waitFor(() => attaching.length === 2);
  assert.equal(attaching.at(-1).length, 200, 'a loader attached after subscription also establishes the window');
  subscription.unsubscribe();
  assert.equal(observers.size, 0);
  console.log('live-query-window-bound smoke OK (871 changes remain a 200-row window; strict authority retained)');
} finally {
  subscription?.unsubscribe();
  await db.close();
}
