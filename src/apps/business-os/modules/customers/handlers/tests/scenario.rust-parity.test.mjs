// Portierung von store_customer_commands.rs:1665 customers_commands_create_pipeline_task_and_activities
// und :1807 customers_invalid_command_writes_failed_projection_without_partial_record.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;

test('Rust-Szenario: Konto → Kontakt → Opportunity → Stufe → Aufgabe → erledigt, ≥ 6 Aktivitäten', async () => {
  const ctx = createCtx();
  await run(ctx, 'customers.account.create', { account_id: 'acct_customers', name: 'Metric Space', domain: 'metric-space.ai', health_status: 'healthy' }, { id: 'cmd_customer_account_create', record_id: 'acct_customers' });
  await run(ctx, 'customers.contact.create', { contact_id: 'contact_customers', account_id: 'acct_customers', first_name: 'Ada', last_name: 'Lovelace', email: 'ada@example.com', is_primary_contact: true }, { id: 'cmd_customer_contact_create' });
  await run(ctx, 'customers.opportunity.create', { opportunity_id: 'opp_customers', name: 'Renewal 2026', account_id: 'acct_customers', primary_contact_id: 'contact_customers', amount_cents: 1200000, opportunity_type: 'renewal' }, { id: 'cmd_customer_opportunity_create' });
  await run(ctx, 'customers.opportunity.move_stage', { opportunity_id: 'opp_customers', stage: 'proposal' }, { id: 'cmd_customer_opportunity_stage' });
  await run(ctx, 'customers.task.create', { task_id: 'task_customers', account_id: 'acct_customers', opportunity_id: 'opp_customers', title: 'Renewal Call vorbereiten' }, { id: 'cmd_customer_task_create' });
  await run(ctx, 'customers.task.complete', { task_id: 'task_customers' }, { id: 'cmd_customer_task_complete' });
  assert.equal(ctx.$.doc('customer_opportunities', 'opp_customers').stage, 'proposal');
  assert.equal(ctx.$.doc('customer_tasks', 'task_customers').status, 'completed');
  assert.ok(ctx.$.all('customer_activities').length >= 6);
  assert.equal(ctx.$.all('customer_activities').length, 6);
});

test('Rust-Szenario: ungültiger health_status → Fehler mit Feldname, kein Teildatensatz', async () => {
  const ctx = createCtx();
  await assert.rejects(
    run(ctx, 'customers.account.create', { account_id: 'acct_invalid', name: 'Invalid GmbH', health_status: 'glowing' }),
    (err) => err.code === 'invalid' && err.message.includes('health_status'),
  );
  assert.equal(ctx.$.doc('customer_accounts', 'acct_invalid'), null);
});

test('Idempotenz: gleicher Befehl zweimal → zweites Mal conflict, Aktivitäts-ID deterministisch', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const a = await run(ctx, 'customers.note.create', { note_id: 'n', account_id: 'acct_1' }, { id: 'same' });
  await assert.rejects(run(ctx, 'customers.note.create', { note_id: 'n', account_id: 'acct_1' }, { id: 'same' }), { code: 'conflict' });
  assert.equal(a.activity.id, 'act_' + stableDigest('same:note_created:n'));
});

test('Rückgaben bleiben unter 1 MiB und sind reines JSON', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, 'customers.account.update', { account_id: 'acct_1', name: 'X' });
  const text = JSON.stringify(out);
  assert.ok(text.length < 1024 * 1024);
  assert.deepEqual(JSON.parse(text), out);
});
