import assert from 'node:assert/strict';
const { createRxDatabase } = await import(
  process.argv.includes('--source') ? '../src/index.mjs' : '../dist/ctox-rxdb-js.mjs'
);

// Hold the real RxQuery's storage reads open; no browser, network, or large
// fixture is needed to reproduce repeated materialization during live writes.
const listeners = new Set();
const reads = [];
let revision = 0;
let inFlight = 0;
let peak = 0;
const storage = {
  observe(listener) { listeners.add(listener); return () => listeners.delete(listener); },
  queryDocuments() {
    inFlight += 1;
    peak = Math.max(peak, inFlight);
    const snapshot = revision;
    return new Promise(resolve => reads.push(() => {
      inFlight -= 1;
      resolve([{ id: 'a', revision: snapshot }]);
    }));
  },
};
const db = await createRxDatabase({
  name: 'query-single-flight',
  storage: { nativeStorage: { collection: () => storage, close() {} } },
});
await db.addCollections(Object.fromEntries(['items', 'business_commands'].map(name => [name, {
  schema: { version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string' }, revision: { type: 'number' } } },
}])));
const emitChange = () => {
  revision += 1;
  for (const listener of listeners) listener({ success: { a: { id: 'a', revision } } });
};
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(predicate) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (predicate()) return;
    await pause(5);
  }
  assert.ok(predicate(), 'bounded wait expired');
}

let subscription;
try {
  const emissions = [];
  subscription = db.items.find({ selector: {}, limit: 2 }).$.subscribe(value => emissions.push(value));
  await waitFor(() => reads.length === 1);
  for (let index = 0; index < 4; index += 1) {
    emitChange();
    await pause(60);
  }
  assert.equal(peak, 1, 'slow bounded live queries must not overlap after change debounce');
  assert.equal(reads.length, 1, 'many invalidations must coalesce behind the current read');
  reads[0]();
  await waitFor(() => emissions.length === 1 && reads.length === 2);
  assert.equal(emissions[0][0].revision, 0, 'first available snapshot must paint despite ongoing writes');
  emitChange();
  await pause(60);
  assert.equal(reads.length, 2, 'the follow-up read is also single-flight');
  reads[1]();
  await waitFor(() => emissions.length === 2 && reads.length === 3);
  assert.equal(emissions[1][0].revision, 4);
  reads[2]();
  await waitFor(() => emissions.length === 3);
  assert.equal(emissions[2][0].revision, 5, 'coalesced follow-up must include the last change');
  await pause(60);
  assert.equal(reads.length, 3, 'no redundant timer read remains after the final follow-up');

  emitChange();
  await waitFor(() => reads.length === 4);
  subscription.unsubscribe();
  reads[3]();
  await pause(60);
  assert.equal(emissions.length, 3, 'unsubscribe fences the pending local result');
  assert.equal(listeners.size, 0, 'unsubscribe releases its storage observer');

  // A control-plane loader replacement must clear the old authorized result,
  // cancel only this subscription's read, and execute the replacement once.
  const commands = db.business_commands;
  let oldSignal;
  let releaseOld;
  let newReads = 0;
  commands.setDemandLoader({ resolveQuery(_query, options) {
    oldSignal = options.signal;
    return new Promise(resolve => { releaseOld = resolve; });
  } });
  const commandEmissions = [];
  subscription = commands.find({ selector: {}, limit: 2, requireRevision: 'current-authority' }).$.subscribe(value => commandEmissions.push(value));
  await waitFor(() => oldSignal);
  commands.setDemandLoader({ async resolveQuery(query) {
    assert.equal(query.requireRevision, 'current-authority', 'strict revision remains mandatory');
    newReads += 1;
    return [{ id: 'new', revision: 2 }];
  } });
  assert.equal(oldSignal.aborted, true, 'loader replacement cancels the owned pending read');
  assert.equal(newReads, 0, 'even an uncooperative old loader cannot overlap the replacement');
  releaseOld([{ id: 'old', revision: 1 }]);
  await waitFor(() => commandEmissions.some(value => value[0]?.id === 'new'));
  assert.equal(newReads, 1);
  assert.ok(!commandEmissions.some(value => value[0]?.id === 'old'), 'replaced authority never emits');
  subscription.unsubscribe();

  // Cancelling a subscription must not cancel another imperative consumer of
  // the same query, or mutate the original caller-owned signal.
  const caller = new AbortController();
  const signals = [];
  const pending = [];
  commands.setDemandLoader({ resolveQuery(_query, options) {
    signals.push(options.signal);
    return new Promise(resolve => pending.push(resolve));
  } });
  const sharedQuery = commands.find({ selector: {}, limit: 2, signal: caller.signal });
  subscription = sharedQuery.$.subscribe(() => { throw new Error('unsubscribed result emitted'); });
  const imperative = sharedQuery.exec();
  await waitFor(() => signals.length === 2);
  subscription.unsubscribe();
  assert.equal(signals[0].aborted, true);
  assert.equal(caller.signal.aborted, false, 'caller-owned signal is not aborted');
  assert.equal(signals[1], caller.signal, 'imperative consumer keeps its own signal');
  pending[0]([]);
  pending[1]([{ id: 'independent', revision: 3 }]);
  assert.equal((await imperative)[0].id, 'independent');
  await pause(0);
  console.log(`query-observable-single-flight smoke OK (peak=${peak}, writes=6, boundedReads=4)`);
} finally {
  subscription?.unsubscribe();
  // Releasing unfinished fake reads does not mutate any real database.
  if (inFlight > 0) for (const finish of reads.slice(-inFlight)) finish();
  await db.close();
}
