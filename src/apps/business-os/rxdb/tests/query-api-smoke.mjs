import { ctoxRxdbTestInternals } from '../dist/ctox-rxdb-js.mjs';

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

console.log('ctox-rxdb-js query API smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
