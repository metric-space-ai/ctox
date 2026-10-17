import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.account.create';

test('legt Konto mit Defaults, search_text und Aktivität an', async () => {
  const ctx = createCtx();
  const out = await run(ctx, T, { account_id: 'acct_x', name: 'Metric Space', domain: 'Metric-Space.AI', health_status: 'healthy' });
  const doc = ctx.$.doc('customer_accounts', 'acct_x');
  assert.equal(doc.account_status, 'active');
  assert.equal(doc.customer_stage, 'active');
  assert.equal(doc.currency, 'EUR');
  assert.equal(doc.ideal_customer_profile, false);
  assert.equal(doc.is_deleted, false);
  assert.equal(doc.created_at_ms, NOW);
  assert.equal(doc.updated_at_ms, NOW);
  assert.equal(doc.search_text, 'metric space metric-space.ai');
  assert.equal(doc.account_id, 'acct_x', 'Rust übernimmt den ganzen Payload');
  assert.equal(out.ok, true);
  assert.equal(out.collection, 'customer_accounts');
  assert.equal(out.account.updated_at_ms, undefined, 'Rückgabe = Stand vor upsert (Rust clone)');
  const actId = 'act_' + stableDigest('cmd_' + T + ':account_created:acct_x');
  assert.equal(out.activity.id, actId);
  const act = ctx.$.doc('customer_activities', actId);
  assert.equal(act.name, 'Kunde erstellt');
  assert.equal(act.actor_id, 'tester');
  assert.equal(act.properties.command_type, T);
  assert.equal(act.properties.command_id, 'cmd_' + T);
  assert.equal(act.contact_id, '');
});

test('ohne IDs: acct_<digest(command.id)>; mit record_id: record_id', async () => {
  const ctx = createCtx();
  const a = await run(ctx, T, { name: 'A' }, { id: 'c1' });
  assert.equal(a.account.id, 'acct_' + stableDigest('c1'));
  const b = await run(ctx, T, { name: 'B' }, { id: 'c2', record_id: ' rec_b ' });
  assert.equal(b.account.id, 'rec_b');
});

test('Fehler: name fehlt, Dublette, unbekannter health_status – ohne Teilschreiben', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await assert.rejects(run(ctx, T, { account_id: 'n' }), { code: 'invalid', message: 'name is required' });
  await assert.rejects(run(ctx, T, { account_id: 'acct_1', name: 'x' }), { code: 'conflict', message: 'customer account already exists' });
  await assert.rejects(run(ctx, T, { account_id: 'acct_invalid', name: 'Invalid GmbH', health_status: 'glowing' }),
    { code: 'invalid', message: 'health_status `glowing` is not supported' });
  assert.equal(ctx.$.doc('customer_accounts', 'acct_invalid'), null);
  assert.equal(ctx.$.all('customer_activities').length, 0);
});

test('Leerstring-Status wird wie in Rust durch Default ersetzt', async () => {
  const ctx = createCtx();
  const out = await run(ctx, T, { account_id: 'a', name: 'A', account_status: '  ' });
  assert.equal(out.account.account_status, 'active');
});
