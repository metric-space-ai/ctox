import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, matchSelector } from './_ctx.mjs';

test('Mango: fehlend ≠ null, $exists, $eq:null', () => {
  assert.equal(matchSelector({ a: null }, { a: { $eq: null } }), true);
  assert.equal(matchSelector({}, { a: { $eq: null } }), false);
  assert.equal(matchSelector({}, { a: { $exists: false } }), true);
  assert.equal(matchSelector({ a: 1 }, { $or: [{ a: 2 }, { a: { $gte: 1 } }] }), true);
  assert.equal(matchSelector({ a: '1' }, { a: { $gt: 0 } }), false);
});

test('read sortiert nach id ohne sort; get liefert eingefrorene Kopie', async () => {
  const ctx = createCtx({ seed: { customer_tasks: [{ id: 'b' }, { id: 'a' }] } });
  const rows = await ctx.read('customer_tasks', { selector: {} });
  assert.deepEqual(rows.map((r) => r.id), ['a', 'b']);
  const doc = await ctx.get('customer_tasks', 'a');
  assert.throws(() => { doc.x = 1; });
  assert.equal(await ctx.get('customer_tasks', 'zz'), null);
});

test('transaction verwirft bei throw alles', async () => {
  const ctx = createCtx();
  await assert.rejects(ctx.transaction(async (tx) => {
    await tx.put('customer_views', {
      id: 'v', name: 'n', object_type: 'account', view_type: 'table', visibility: 'private', is_deleted: false,
      created_at_ms: 1, updated_at_ms: 1,
    });
    assert.ok(await tx.get('customer_views', 'v'));
    throw { code: 'invalid', message: 'x' };
  }), { code: 'invalid' });
  assert.equal(ctx.$.doc('customer_views', 'v'), null);
});

test('Schemaprüfung und Fremd-Collections', async () => {
  const ctx = createCtx();
  await assert.rejects(ctx.put('customer_tasks', { id: 't' }), { code: 'invalid' });
  await assert.rejects(ctx.put('outbound_companies', { id: 'c' }), { code: 'forbidden' });
  await assert.rejects(ctx.read('invoices', {}), { code: 'forbidden' });
  assert.deepEqual(await ctx.read('outbound_companies', {}), []);
  await ctx.cap.legacy['customers.import.file']({ a: 1 });
  assert.deepEqual(ctx.$.capCalls, [{ cap: 'legacy.customers.import.file', args: [{ a: 1 }] }]);
});
