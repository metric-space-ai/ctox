import { createRxDatabase, ctoxRxdbTestInternals } from '../dist/ctox-rxdb-js.mjs';

const {
  matchesSelector,
  normalizeQuery,
  normalizeSort,
  sortDocuments,
} = ctoxRxdbTestInternals;

const docs = [
  { id: 'a', title: 'Alpha', status: 'open', tags: ['one', 'two'], score: 2, nested: { value: 3 }, rows: [{ kind: 'x', qty: 2 }] },
  { id: 'b', title: 'Beta', status: 'done', tags: ['two'], score: 5, nested: { value: 9 }, rows: [{ kind: 'y', qty: 7 }] },
  { id: 'c', title: 'Gamma', status: 'open', tags: [], score: 1, nested: { value: 1 }, rows: [] },
];

assert(matchesSelector(docs[0], { status: 'open', score: { $gte: 2, $lte: 2 } }), 'basic selector failed');
assert(matchesSelector(docs[1], { status: { $in: ['done'] }, tags: { $contains: 'two' } }), 'array selector failed');
assert(matchesSelector(docs[0], { $and: [{ status: 'open' }, { title: { $regex: '^Al' } }] }), '$and/$regex selector failed');
assert(matchesSelector(docs[0], { $or: [{ status: 'missing' }, { 'nested.value': { $gt: 2 } }] }), '$or/nested selector failed');
assert(matchesSelector(docs[1], { rows: { $elemMatch: { kind: 'y', qty: { $gte: 7 } } } }), '$elemMatch selector failed');
assert(!matchesSelector(docs[2], { $not: { status: 'open' } }), '$not selector failed');

const byScoreDesc = sortDocuments(docs, normalizeSort([{ score: 'desc' }])).map((doc) => doc.id).join(',');
assert(byScoreDesc === 'b,a,c', `sort desc mismatch: ${byScoreDesc}`);

const byTitleAsc = sortDocuments(docs, normalizeSort('title')).map((doc) => doc.id).join(',');
assert(byTitleAsc === 'a,b,c', `sort string mismatch: ${byTitleAsc}`);

const directIdQuery = normalizeQuery('doc-1', 'id');
assert(directIdQuery.selector.id === 'doc-1', 'string findOne query normalization failed');

const mangoQuery = normalizeQuery({ selector: { status: 'open' }, sort: [{ score: -1 }], skip: 1, limit: 2 }, 'id');
assert(mangoQuery.skip === 1 && mangoQuery.limit === 2 && mangoQuery.sort[0].score === 'desc', 'mango query normalization failed');

const countCalls = [];
const db = await createRxDatabase({
  name: 'query-api-fake',
  storage: {
    nativeStorage: {
      collection(name) {
        return {
          name,
          schemaIndexes: () => [],
          observe: () => () => {},
          countDocuments: async (query) => {
            countCalls.push(query);
            return 7;
          },
          queryDocuments: async () => {
            throw new Error('count() must not materialize find().exec()');
          },
          allDocuments: async () => {
            throw new Error('count() must not call allDocuments()');
          },
        };
      },
      close() {},
    },
  },
});
await db.addCollections({
  items: {
    schema: {
      version: 0,
      primaryKey: 'id',
      type: 'object',
      properties: { id: { type: 'string' }, status: { type: 'string' } },
    },
  },
});
const delegatedCount = await db.items.count({ selector: { status: 'open' }, limit: 10 }).exec();
assert(delegatedCount === 7, `countDocuments delegation returned ${delegatedCount}`);
assert(countCalls.length === 1, `countDocuments must be called once (got ${countCalls.length})`);
assert(countCalls[0].selector.status === 'open', 'countDocuments receives normalized selector');

const liveStorage = createLiveStorage([
  { id: 'a', title: 'Alpha', status: 'open' },
]);
const liveDb = await createRxDatabase({
  name: 'query-live-delta-fake',
  storage: {
    nativeStorage: {
      collection() {
        return liveStorage;
      },
      close() {},
    },
  },
});
await liveDb.addCollections({
  live_items: {
    schema: {
      version: 0,
      primaryKey: 'id',
      type: 'object',
      properties: { id: { type: 'string' }, title: { type: 'string' } },
    },
  },
});

const collectionEmissions = [];
const collectionSub = liveDb.live_items.$.subscribe((value) => {
  collectionEmissions.push(value);
});
await waitFor(() => collectionEmissions.length === 1);
assert(liveStorage.stats.queryCalls === 1, `collection.$ initial read must query once (got ${liveStorage.stats.queryCalls})`);
liveStorage.emitChange({
  b: { id: 'b', title: 'Beta', status: 'open' },
});
await waitFor(() => collectionEmissions.length === 2);
assert(liveStorage.stats.queryCalls === 1, 'collection.$ must apply change deltas without full query re-exec');
assert(collectionEmissions.at(-1).documents.map((doc) => doc.id).sort().join(',') === 'a,b', 'collection.$ delta must include created doc');
collectionSub.unsubscribe();

const findOneEmissions = [];
const findOneSub = liveDb.live_items.findOne('a').$.subscribe((value) => {
  findOneEmissions.push(value);
});
await waitFor(() => findOneEmissions.length === 1);
assert(liveStorage.stats.queryCalls === 2, `findOne.$ initial read must query once (got total ${liveStorage.stats.queryCalls})`);
liveStorage.emitChange({
  b: { id: 'b', title: 'Beta updated', status: 'open' },
});
await sleep(75);
assert(findOneEmissions.length === 1, 'findOne(primary).$ must ignore unrelated changed ids');
assert(liveStorage.stats.queryCalls === 2, 'findOne(primary).$ must not re-query for unrelated changed ids');
liveStorage.emitChange({
  a: { id: 'a', title: 'Alpha updated', status: 'open' },
});
await waitFor(() => findOneEmissions.length === 2);
assert(findOneEmissions.at(-1).title === 'Alpha updated', 'findOne(primary).$ must emit the changed primary doc');
assert(liveStorage.stats.queryCalls === 2, 'findOne(primary).$ must apply primary-key deltas without query re-exec');
liveStorage.emitChange({
  a: { id: 'a', title: 'Alpha deleted', _deleted: true },
});
await waitFor(() => findOneEmissions.length === 3);
assert(findOneEmissions.at(-1) === null, 'findOne(primary).$ must emit null for deleted primary doc');
assert(liveStorage.stats.queryCalls === 2, 'findOne(primary).$ deletion delta must not re-query');
findOneSub.unsubscribe();

liveDb.live_items.resetQueryPerformanceStats();
const complexEmissions = [];
const complexSub = liveDb.live_items.find({ selector: { status: 'open' } }).$.subscribe((value) => {
  complexEmissions.push(value);
});
await waitFor(() => complexEmissions.length === 1);
const complexInitialQueryCalls = liveStorage.stats.queryCalls;
liveStorage.emitChange({
  c: { id: 'c', title: 'Gamma', status: 'open' },
});
await waitFor(() => complexEmissions.length === 2);
assert(
  liveStorage.stats.queryCalls === complexInitialQueryCalls,
  'unbounded live query must apply matching changed docs without full query re-exec',
);
assert(
  complexEmissions.at(-1).map((doc) => doc.id).sort().join(',') === 'b,c',
  'unbounded live query delta must include existing and newly matching docs',
);
liveStorage.emitChange({
  b: { id: 'b', title: 'Beta closed', status: 'done' },
});
await waitFor(() => complexEmissions.length === 3);
assert(
  liveStorage.stats.queryCalls === complexInitialQueryCalls,
  'unbounded live query must remove non-matching changed docs without full query re-exec',
);
assert(
  complexEmissions.at(-1).map((doc) => doc.id).join(',') === 'c',
  'unbounded live query delta must remove docs that no longer match',
);
const queryStats = liveDb.live_items.getQueryPerformanceStats();
assert(
  queryStats.liveQueries.complexLiveQueryReexecs === 0,
  `unbounded live query must not re-exec after delta changes: ${queryStats.liveQueries.complexLiveQueryReexecs}`,
);
assert(
  queryStats.liveQueries.deltaLiveQueryApplies === 2,
  `delta live query apply counter mismatch: ${queryStats.liveQueries.deltaLiveQueryApplies}`,
);
assert(
  queryStats.liveQueries.lastDeltaLiveQuery?.selectorFields?.join(',') === 'status',
  'delta live query counter must retain selector fields for attribution',
);
complexSub.unsubscribe();
await liveDb.close();

console.log('ctox-rxdb-js query API smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function createLiveStorage(seedDocs = []) {
  const docs = new Map(seedDocs.map((doc) => [doc.id, { ...doc }]));
  const listeners = new Set();
  const stats = { queryCalls: 0 };
  return {
    stats,
    schemaIndexes: () => [],
    observe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async queryDocuments(query = {}) {
      stats.queryCalls += 1;
      const selector = query.selector || {};
      let values = Array.from(docs.values()).filter((doc) => !doc._deleted);
      if (selector.id) {
        values = values.filter((doc) => doc.id === selector.id);
      }
      return values.map((doc) => ({ ...doc }));
    },
    emitChange(success = {}) {
      for (const doc of Object.values(success)) {
        if (doc?._deleted) {
          docs.delete(doc.id);
        } else {
          docs.set(doc.id, { ...doc });
        }
      }
      for (const listener of [...listeners]) {
        listener({ success });
      }
    },
  };
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(predicate, { timeoutMs = 500, intervalMs = 10 } = {}) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await sleep(intervalMs);
  }
  assert(predicate(), 'timed out waiting for condition');
}
