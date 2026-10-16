import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.import.from_outbound';
const outbound = () => ({
  outbound_companies: [
    { id: 'company_one', campaign_id: 'camp', name: 'Acme GmbH', domain: 'acme.example', website: 'https://acme.example', payload: {}, created_at_ms: 1 },
    { id: 'company_duplicate', campaign_id: 'camp', name: 'Acme Duplicate', domain: 'ACME.example', payload: {}, created_at_ms: 2 },
    { id: 'company_web', name: 'Web Only', website: 'www.web-only.example/about', payload: {} },
    { id: 'company_noname', website: 'noname.example' },
  ],
  outbound_pipeline_items: [
    { id: 'pipeline_one', campaign_id: 'camp', company_id: 'company_one', company_name: 'Acme GmbH', stage: 'qualified',
      contacts: [{ name: 'Grace Hopper', email: 'grace@acme.example', job_title: 'CTO' }, { name: 'Alan', title: 'CEO', id: 'p2' }], payload: {} },
    { id: 'pipeline_noname', company_id: 'company_noname', company_name: 'Aus Pipeline' },
  ],
});

test('Rust-Testfall: Import legt Konto + Kontakte an, Dublette → needs_review, dann resolve', async () => {
  const ctx = createCtx({ seed: outbound() });
  const out = await run(ctx, T, { source_record_id: 'company_one', pipeline_id: 'pipeline_one', account_id: 'acct_acme' }, { id: 'cmd_customer_import', record_id: 'company_one' });
  assert.equal(out.status, 'imported');
  const account = ctx.$.doc('customer_accounts', 'acct_acme');
  assert.equal(account.name, 'Acme GmbH');
  assert.equal(account.source, 'outbound');
  assert.equal(account.search_text, 'acme gmbh acme.example');
  assert.equal(account.payload.outbound_pipeline.id, 'pipeline_one');
  const contacts = ctx.$.all('customer_contacts').filter((c) => c.account_id === 'acct_acme');
  assert.equal(contacts.length, 2);
  const grace = contacts.find((c) => c.email === 'grace@acme.example');
  assert.equal(grace.id, 'contact_' + stableDigest('acct_acme:0:' + JSON.stringify(outbound().outbound_pipeline_items[0].contacts[0])));
  assert.deepEqual([grace.first_name, grace.last_name, grace.job_title, grace.is_primary_contact], ['Grace', 'Hopper', 'CTO', true]);
  const alan = contacts.find((c) => c.first_name === 'Alan');
  assert.deepEqual([alan.last_name, alan.job_title, alan.source_record_id, alan.is_primary_contact], ['', 'CEO', 'p2', false]);
  // Batch-ID: kein import_batch_id → record_id (company_one), wie in Rust
  const batch = ctx.$.doc('customer_import_batches', 'company_one');
  assert.equal(batch.imported_count, 3);
  assert.equal(batch.status, 'completed');
  assert.equal(out.activity.name, 'Outbound-Uebergabe importiert');
  assert.equal(out.activity.properties.contact_count, 2);

  const dup = await run(ctx, T, { source_record_id: 'company_duplicate' }, { id: 'cmd_customer_import_duplicate', record_id: 'company_duplicate' });
  assert.equal(dup.status, 'needs_review');
  assert.equal(dup.dedupe_candidate.id, 'dedupe_' + stableDigest('outbound:company_duplicate:acct_acme'));
  assert.equal(dup.dedupe_candidate.match_key, 'ACME.example');
  assert.equal(dup.import_batch.dedupe_count, 1);
  assert.equal(dup.activity, undefined);
  assert.equal(ctx.$.all('customer_accounts').length, 1);

  await run(ctx, 'customers.dedupe.resolve', { candidate_id: dup.dedupe_candidate.id, decision: 'keep_existing' });
  assert.equal(ctx.$.all('customer_dedupe_candidates').filter((c) => c.status === 'resolved').length, 1);
});

test('Domain aus Website, Pipeline per company_id gefunden, Name aus Pipeline', async () => {
  const ctx = createCtx({ seed: outbound() });
  const web = await run(ctx, T, { outbound_company_id: 'company_web' }, { id: 'w' });
  assert.equal(web.account.domain, 'web-only.example');
  assert.equal(web.account.id, 'acct_' + stableDigest('w'));
  assert.equal(web.import_batch.id, 'import_' + stableDigest('w'));
  assert.equal(web.account.payload.outbound_pipeline, null);
  const nn = await run(ctx, T, { company_id: 'company_noname' }, { id: 'n' });
  assert.equal(nn.account.name, 'Aus Pipeline');
  assert.equal(nn.account.payload.outbound_pipeline.id, 'pipeline_noname');
});

test('Fehler: keine Quelle, unbekannte Firma, kein Name', async () => {
  const seed = outbound();
  seed.outbound_pipeline_items.pop();
  const ctx = createCtx({ seed });
  await assert.rejects(run(ctx, T, {}), { code: 'invalid', message: 'source_record_id or outbound company_id is required' });
  await assert.rejects(run(ctx, T, { source_record_id: 'ghost' }), { code: 'not_found', message: 'outbound company not found' });
  await assert.rejects(run(ctx, T, { source_record_id: 'company_noname' }), { code: 'invalid', message: 'outbound company name is required' });
  assert.equal(ctx.$.writes.length, 0);
});

test('ohne foreign_collections-Deklaration → forbidden', async () => {
  const ctx = createCtx({ seed: outbound(), foreign: {} });
  await assert.rejects(run(ctx, T, { source_record_id: 'company_one' }), { code: 'forbidden' });
});

test('archiviertes Konto zählt als Dubletten-Treffer (Rust: nur deleted-Spalte)', async () => {
  const seed = { ...outbound(), ...seedBasic() };
  seed.customer_accounts[0] = { ...seed.customer_accounts[0], domain: 'acme.example', is_deleted: true };
  const ctx = createCtx({ seed });
  const out = await run(ctx, T, { source_record_id: 'company_one' });
  assert.equal(out.status, 'needs_review');
  assert.equal(out.dedupe_candidate.existing_record_id, 'acct_1');
});
