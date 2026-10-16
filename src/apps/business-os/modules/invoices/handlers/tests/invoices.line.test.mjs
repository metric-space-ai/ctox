import assert from 'node:assert/strict';
import test from 'node:test';
import create from '../invoices.invoice.create.js';
import post from '../invoices.invoice.post.js';
import lineCreate from '../invoices.line.create.js';
import lineDelete from '../invoices.line.delete.js';
import lineUpdate from '../invoices.line.update.js';
import { cmd, rejects } from './_ctx.mjs';
import { draftPayload, line, setup } from './_fixtures.mjs';

test('line.create/update/delete auf dem Entwurf', async () => {
  const t = await setup();
  await create(cmd('invoices.invoice.create', draftPayload()), t.ctx);
  const c = await lineCreate(cmd('invoices.line.create', { invoice_id: 'inv_post', line: line({ id: 'l2', description: 'Reise' }) }), t.ctx);
  assert.equal(c.invoice.lines.length, 2);
  const u = await lineUpdate(cmd('invoices.line.update', { invoice_id: 'inv_post', line_index: 1, line: { quantity: 2000 } }), t.ctx);
  assert.deepEqual([u.invoice.lines[1].quantity, u.invoice.lines[1].description], [2000, 'Reise']);
  const r = await lineUpdate(cmd('invoices.line.update', { invoice_id: 'inv_post', line_index: 0, line: 'ersetzt' }), t.ctx);
  assert.equal(r.invoice.lines[0], 'ersetzt');
  const d = await lineDelete(cmd('invoices.line.delete', { invoice_id: 'inv_post', line_index: 0 }), t.ctx);
  assert.deepEqual(d.invoice.lines.map((l) => l.id), ['l2']);
});

test('line.*: Pflichtfelder, Bereich, nur Entwurf', async () => {
  const t = await setup();
  await create(cmd('invoices.invoice.create', draftPayload()), t.ctx);
  await rejects(lineCreate(cmd('invoices.line.create', { invoice_id: 'inv_post' }), t.ctx), 'invalid', /requires a 'line' object/);
  await rejects(lineCreate(cmd('invoices.line.create', { invoice_id: 'inv_post', line: [1] }), t.ctx), 'invalid', /must be an object/);
  await rejects(lineUpdate(cmd('invoices.line.update', { invoice_id: 'inv_post', line: {} }), t.ctx), 'invalid', /line_index/);
  await rejects(lineUpdate(cmd('invoices.line.update', { invoice_id: 'inv_post', line_index: -1, line: {} }), t.ctx), 'invalid', /line_index/);
  await rejects(lineDelete(cmd('invoices.line.delete', { invoice_id: 'inv_post', line_index: 5 }), t.ctx), 'invalid', /out of range \(1 lines\)/);
  await rejects(lineCreate(cmd('invoices.line.create', { id: 'inv_post', line: {} }), t.ctx), 'invalid', /missing required field.*\["invoice_id"\]/);
  await post(cmd('invoices.invoice.post', { invoice_id: 'inv_post' }), t.ctx);
  await rejects(lineCreate(cmd('invoices.line.create', { invoice_id: 'inv_post', line: line() }), t.ctx), 'conflict', /state=draft/);
});
