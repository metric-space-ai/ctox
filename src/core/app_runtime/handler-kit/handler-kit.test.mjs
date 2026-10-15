import { test } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { createHash } from 'node:crypto';
import { createContext, runHandler, runMigration, mango, CtoxError } from './index.mjs';

const manifest = overrides => ({ app_id: 'test-app', collections: { records: {} }, handlers: { 'test.run': 'handler.mjs' }, capabilities_required: [], ...overrides });
const denied = code => error => error instanceof CtoxError && error.code === code;
const docs = [
  { id: 'b', n: 2, title: 'Beta', nullable: null, tags: ['b', 'c'], items: [{ qty: 2 }] },
  { id: 'a', n: 1, title: 'alpha', tags: ['a'], items: [{ qty: 1 }] },
  { id: 'c', n: 2, title: 'alpha', nullable: 3 },
];
const ids = selector => mango(docs, { selector }).map(doc => doc.id);
test('Mango distinguishes missing, null and exists and handles equality/inequality', () => {
  assert.deepEqual(ids({ nullable: { $eq: null } }), ['b']);
  assert.deepEqual(ids({ nullable: { $exists: false } }), ['a']);
  assert.deepEqual(ids({ n: { $eq: 2, $ne: 1 } }), ['b', 'c']);
  assert.deepEqual(ids({ nullable: { $exists: true } }), ['b', 'c']);
});
test('Mango numeric comparison operators', () => {
  assert.deepEqual(ids({ n: { $gt: 1, $gte: 2, $lt: 3, $lte: 2 } }), ['b', 'c']);
  assert.deepEqual(ids({ absent: { $gt: 0 } }), []);
});
test('Mango membership, logical selectors and field not', () => {
  assert.deepEqual(ids({ n: { $in: [1, 2], $nin: [1] } }), ['b', 'c']);
  assert.deepEqual(ids({ $and: [{ n: 2 }, { $or: [{ id: 'b' }, { id: 'c' }] }], $not: { id: 'b' } }), ['c']);
  assert.deepEqual(ids({ n: { $not: { $eq: 2 } } }), ['a']);
});
test('Mango regex permits i only and rejects malformed queries even against empty data', () => {
  assert.deepEqual(ids({ title: { $regex: '^ALP', $options: 'i' } }), ['a', 'c']);
  for (const selector of [{ title: { $regex: '[', $options: 'i' } }, { n: { $wat: 1 } }, { title: { $regex: 'x', $options: 'g' } }, { title: { $options: 'i' } }])
    assert.throws(() => mango([], { selector }), denied('invalid'));
});
test('Mango elemMatch supports scalar conditions, object selectors, and only one level', () => {
  assert.deepEqual(ids({ tags: { $elemMatch: { $eq: 'b' } } }), ['b']);
  assert.deepEqual(ids({ items: { $elemMatch: { qty: { $gte: 2 } } } }), ['b']);
  assert.throws(() => mango([], { selector: { items: { $elemMatch: { deep: { $elemMatch: { x: 1 } } } } } }), denied('invalid'));
});
test('Mango multi-field ordering, id ties, pagination, no default row cap or result alias', () => {
  assert.deepEqual(mango(docs, { selector: {}, sort: [{ n: 'desc' }, { title: 'asc' }], skip: 1, limit: 1 }).map(doc => doc.id), ['c']);
  assert.deepEqual(mango(docs, { selector: {}, sort: [{ n: 'desc' }] }).map(doc => doc.id), ['b', 'c', 'a']);
  const many = Array.from({ length: 1200 }, (_, n) => ({ id: String(n) }));
  assert.equal(mango(many, { selector: {} }).length, 1200);
  const read = mango(docs, { selector: {} }); read[0].n = 999;
  assert.equal(docs[1].n, 1);
  for (const query of [{ selector: null }, { selector: {}, sort: null }, { selector: { n: { $not: 1 } } }, { selector: {}, limit: -1 }, { selector: {}, skip: 0.5 }, { selector: {}, sort: [{ n: 'up' }] }, { selector: { '__proto__.x': 1 } }]) assert.throws(() => mango([], query), denied('invalid'));
});
test('ctx read/write runs on the in-memory store and enforces compare-and-set', async () => {
  const fixtures = { collections: { records: [{ id: 'one', rev: 'old', n: 1 }] } };
  const ctx = createContext(manifest(), fixtures);
  const result = await ctx.write('records', { type: 'patch', id: 'one', base_rev: 'old', patch: { n: 2 } });
  assert.equal(result.status, 'accepted');
  assert.equal((await ctx.read('records', { selector: { n: 2 } }))[0].id, 'one');
  await assert.rejects(ctx.write('records', { type: 'remove', id: 'one', base_rev: 'old' }), denied('conflict'));
  await assert.rejects(ctx.write('records', { type: 'patch', id: 'one', patch: { id: 'other' } }), denied('invalid'));
  await ctx.write('records', { type: 'remove', id: 'one', base_rev: result.rev });
  assert.deepEqual(fixtures.collections.records, []);
});
test('foreign collection needs declaration, actual owner, exports and the requested operation', async () => {
  const owner = { app_id: 'owner', collections: { shared: {} }, exports: { shared: ['read'] } };
  const app = manifest({ foreign_collections: { shared: { owner_app: 'owner', ops: ['read', 'write'] } } });
  const fixtures = { collections: { shared: [{ id: 'x' }] }, apps: { owner } };
  const ctx = createContext(app, fixtures);
  assert.equal((await ctx.read('shared', { selector: {} })).length, 1);
  await assert.rejects(ctx.write('shared', { type: 'put', doc: { id: 'x' } }), denied('forbidden'));
  owner.exports.shared = [];
  await assert.rejects(ctx.read('shared', { selector: {} }), denied('forbidden'));
  await assert.rejects(createContext(manifest(), fixtures).read('shared'), denied('forbidden'));
  fixtures.apps.owner = { ...owner, app_id: 'forged', exports: { shared: ['read'] } };
  await assert.rejects(ctx.read('shared'), denied('forbidden'));
});
test('actor policy is checked for every read and write and sees actor attribution', async () => {
  let allowed = true; const seen = [];
  const fixtures = { actor: { id: 'actor' }, policy: request => { seen.push(request); return allowed; } };
  const ctx = createContext(manifest(), fixtures);
  await ctx.read('records');
  allowed = false;
  await assert.rejects(ctx.write('records', { type: 'put', doc: { id: 'x' } }), denied('forbidden'));
  assert.equal(seen[0].actor.id, 'actor');
  assert.deepEqual(fixtures.collections.records, []);
});
test('declared and approved capabilities support namespaced methods and live grant withdrawal', async () => {
  const app = manifest({ capabilities_required: ['document.render'] });
  const fixtures = { grants: ['document.render'], capabilities: { document: { render: arg => ({ pages: arg.pages }) } } };
  const ctx = createContext(app, fixtures);
  assert.deepEqual(await ctx.cap.document.render({ pages: 2 }), { pages: 2 });
  await assert.rejects(ctx.cap.document.delete({}), denied('forbidden'));
  fixtures.grants.length = 0;
  await assert.rejects(ctx.cap.document.render({}), denied('forbidden'));
});
test('model, HTTP and commands use explicit fixture callbacks and log app attribution', async () => {
  const calls = [];
  const fixtures = {
    grants: ['model', 'http'], model: req => { calls.push(req); return { answer: 'fixture' }; },
    http: req => ({ status: req.method === 'GET' ? 200 : 400 }),
    command: (type, payload) => ({ type, payload }),
  };
  const ctx = createContext(manifest({ capabilities_required: ['model', 'http'] }), fixtures);
  assert.deepEqual(await ctx.model({ prompt: 'hello' }), { answer: 'fixture' });
  assert.equal((await ctx.http({ method: 'GET' })).status, 200);
  assert.deepEqual(await ctx.command('test.run', { n: 1 }), { type: 'test.run', payload: { n: 1 } });
  await assert.rejects(ctx.command('foreign.run', {}), denied('forbidden'));
  await assert.rejects(createContext(manifest(), fixtures).http({}), denied('forbidden'));
  ctx.log('info', 'done', { count: calls.length });
  assert.equal(fixtures.logs[0].app_id, 'test-app');
});
async function fixture(t, source = 'export default async (input, ctx) => input;') {
  const root = await fs.mkdtemp(path.join(process.env.TMPDIR ?? os.tmpdir(), 'ctox-handler-kit-'));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  await fs.writeFile(path.join(root, 'handler.mjs'), source);
  return { root };
}
test('runHandler loads the declared module and reads/writes through ctx', async t => {
  const fixtures = await fixture(t, `export default async (input, ctx) => {
    await ctx.write('records', {type:'put', doc:{id:input.id, value:7}});
    return await ctx.read('records', {selector:{id:input.id}});
  };`);
  const result = await runHandler(manifest(), 'handler.mjs', { id: 'new' }, { fixtures });
  assert.equal(result[0].value, 7);
  assert.equal(fixtures.collections.records[0].id, 'new');
  await assert.rejects(runHandler(manifest(), 'other.mjs', {}, { fixtures }), denied('forbidden'));
});
test('handler paths cannot escape through traversal or symlinks', async t => {
  const fixtures = await fixture(t);
  const outer = await fs.mkdtemp(path.join(process.env.TMPDIR ?? os.tmpdir(), 'ctox-handler-foreign-'));
  t.after(() => fs.rm(outer, { recursive: true, force: true }));
  await fs.writeFile(path.join(outer, 'outside.mjs'), 'export default () => null');
  await fs.symlink(path.join(outer, 'outside.mjs'), path.join(fixtures.root, 'link.mjs'));
  await assert.rejects(runHandler(manifest({ handlers: { x: '../outside.mjs' } }), '../outside.mjs', {}, { fixtures }), denied('invalid'));
  await assert.rejects(runHandler(manifest({ handlers: { x: 'link.mjs' } }), 'link.mjs', {}, { fixtures }), denied('forbidden'));
});
test('hash verification and hot reload do not return an older cached handler', async t => {
  const fixtures = await fixture(t, 'export default () => 1;');
  const hash = text => createHash('sha256').update(text).digest('hex');
  const app = manifest({ files: { 'handler.mjs': hash('export default () => 1;') } });
  assert.equal(await runHandler(app, 'handler.mjs', null, { fixtures }), 1);
  await fs.writeFile(path.join(fixtures.root, 'handler.mjs'), 'export default () => 2;');
  await assert.rejects(runHandler(app, 'handler.mjs', null, { fixtures }), denied('conflict'));
  app.files['handler.mjs'] = hash('export default () => 2;');
  assert.equal(await runHandler(app, 'handler.mjs', null, { fixtures }), 2);
});
test('missing native capability fails before executing app handler', async t => {
  const fixtures = await fixture(t, 'throw new Error("must not load")');
  const app = manifest({ capabilities_required: ['document.render'] });
  await assert.rejects(runHandler(app, 'handler.mjs', {}, { fixtures }), denied('forbidden'));
  fixtures.grants = ['document.render'];
  await assert.rejects(runHandler(app, 'handler.mjs', {}, { fixtures }), denied('unavailable'));
});
test('final handler results enforce the one MiB budget', async t => {
  const fixtures = await fixture(t, 'export default () => "x".repeat(1024*1024);');
  await assert.rejects(runHandler(manifest(), 'handler.mjs', {}, { fixtures }), denied('quota'));
});
test('migration returns base-revision proposals and archives without mutating the source', async t => {
  const fixtures = await fixture(t, `export default doc => doc.retired ? {archive:true} : {...doc, title:doc.old_title};`);
  const app = manifest({ migrations: { records: { from_ver: 1, migrate_fn: 'handler.mjs' } } });
  const source = [{ id: 'a', rev: 'r1', old_title: 'kept' }, { id: 'b', rev: 'r2', retired: true }];
  const before = structuredClone(source);
  const result = await runMigration(app, 'records', source, { fixtures });
  assert.equal(result[0].base_rev, 'r1');
  assert.equal(result[0].doc.title, 'kept');
  assert.deepEqual(result[1], { id: 'b', base_rev: 'r2', action: 'archive', archived_doc: before[1] });
  assert.deepEqual(source, before);
});
test('migration cannot write, dispatch commands, call capabilities or change ids', async t => {
  const fixtures = await fixture(t);
  const app = manifest({ migrations: { records: { migrate_fn: 'handler.mjs' } } });
  for (const body of [
    "await ctx.write('records',{type:'put',doc:{id:'x'}})",
    "await ctx.command('test.run',{})", "await ctx.model({})",
    "({ ...doc, id:'other' })",
  ]) {
    await fs.writeFile(path.join(fixtures.root, 'handler.mjs'), 'export default async (doc,ctx) => ' + body + ';');
    await assert.rejects(runMigration(app, 'records', [{ id: 'x', rev: 'r' }], { fixtures }), error => ['forbidden', 'invalid'].includes(error.code));
  }
});
test('migration rejects missing source revisions and duplicate ids', async t => {
  const fixtures = await fixture(t);
  const app = manifest({ migrations: { records: { migrate_fn: 'handler.mjs' } } });
  await assert.rejects(runMigration(app, 'records', [{ id: 'x' }], { fixtures }), denied('invalid'));
  await assert.rejects(runMigration(app, 'records', [{ id: 'x', rev: 'r' }, { id: 'x', rev: 'r' }], { fixtures }), denied('invalid'));
});

test('JS regex query forms normalize to the wire subset and preserve only flag i', () => {
  assert.deepEqual(ids({ title: /^ALP/i }), ['a', 'c']);
  assert.deepEqual(ids({ title: { $regex: /^ALP/i } }), ['a', 'c']);
  assert.deepEqual(ids({ title: { $not: /^ALP/i } }), ['b']);
  assert.deepEqual(ids({ title: { $regex: '^b', $options: '' } }), []);
  for (const selector of [{ title: /alpha/g }, { title: { $regex: /alpha/m } }])
    assert.throws(() => mango([], { selector }), denied('invalid'));
  assert.equal(new CtoxError('invalid', 'field', { path: 'records.title' }).path, 'records.title');
});
