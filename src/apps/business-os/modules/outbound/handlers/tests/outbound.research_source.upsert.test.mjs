import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.research_source.upsert.js';

test('upsert creates adapter record with defaults, no credential needed', async () => {
  const t = createTestCtx({ now: 4 });
  const result = await handle(cmd('outbound.research_source.upsert', { adapter: { id: 'northdata', label: 'North Data', countries: ['DE'] } }, { id: 'c1' }), t.ctx);
  const doc = t.doc('outbound_research_adapters', 'northdata');
  assert.equal(result.command_effect, 'active');
  assert.equal(doc.status, 'active');
  assert.equal(doc.source_id, 'northdata');
  assert.equal(doc.url, 'https://northdata/');
  assert.equal(doc.adapter_kind, 'custom_url');
  assert.equal(doc.scrape_status, 'target_available');
  assert.equal(doc.auth_status, 'not_required');
  assert.equal(doc.enabled, true);
  assert.deepEqual(doc.countries, ['DE']);
  assert.equal(doc.payload.last_command_id, 'c1');
  assert.equal(doc.payload.credential_ref.required, false);
});

test('credential status via secrets capability; writeback scoped to source module', async () => {
  const t = createTestCtx({ caps: { 'secrets.exists': async ({ name }) => ({ exists: name === 'dnb' }) } });
  const command = cmd('outbound.research_source.upsert', {
    adapter_id: 'a1', source_id: 'dnb', requires_credential: true, credential_secret_name: 'dnb',
    writeback: { collection: 'outbound_lead_generation_sources' },
  }, { client_context: { source_module: 'outbound-lead-generation' } });
  await handle(command, t.ctx);
  assert.equal(t.doc('outbound_research_adapters', 'a1').auth_status, 'credential_available');
  assert.equal(t.doc('outbound_lead_generation_sources', 'a1').last_command_id, command.id);

  const bad = cmd('outbound.research_source.upsert', { adapter_id: 'a2', source_id: 's', writeback: { collection: 'customers_records' } }, { client_context: { source_module: 'outbound-lead-generation' } });
  assert.equal((await rejects(handle(bad, t.ctx))).code, 'forbidden');
  assert.equal(t.doc('outbound_research_adapters', 'a2'), null);
  assert.equal((await rejects(handle(cmd('outbound.research_source.upsert', {}), t.ctx))).code, 'invalid');
});
