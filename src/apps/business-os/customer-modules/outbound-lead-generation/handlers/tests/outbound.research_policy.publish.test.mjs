import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import handle, { appSlug } from '../outbound.research_policy.publish.js';

const caps = {};
const TARGETS = 'outbound_lead_generation_scrape_targets';

test('publishes policy target and mirrors record (ref :4529)', async () => {
  const t = createTestCtx({ now: 4242, caps });
  const result = await handle(cmd('outbound.research_policy.publish', {
    app: 'outbound-lead-generation', skill: 'outbound-lead-generation-research', fields: ['name'], min_independent_sources: 1,
  }), t.ctx);
  assert.deepEqual(result, {
    ok: true, app: 'outbound-lead-generation', target_key: 'outbound-lead-generation-policy',
    manifest_path: `${TARGETS}/outbound-lead-generation-policy`, record_id: 'policy:outbound-lead-generation',
  });
  // Registry document = the former manifest (registry.rs:103 upsert_target), no Core capability.
  assert.equal(t.calls.caps.length, 0);
  const m = t.doc(TARGETS, 'outbound-lead-generation-policy');
  assert.match(m.target_id, /^scrape_target-[0-9a-f]{16}$/);
  assert.equal(m.target_kind, 'app-policy');
  assert.equal(m.status, 'active');
  assert.equal(m.start_url, 'https://ctox.local/app-policy');
  assert.equal(m.display_name, 'outbound-lead-generation · Rechercheablauf und Einstellungen');
  assert.deepEqual(m.output_schema, { type: 'object' });
  assert.equal(m.config.published_at_ms, 4242);
  assert.equal(m.config.policy_contract, 'ctox.outbound.research_policy.v1');
  assert.equal(m.config.sources[0].source_key, 'primary');
  const rec = t.doc('outbound_research_policies', 'policy:outbound-lead-generation');
  assert.equal(rec.skill, 'outbound-lead-generation-research');
  assert.equal(rec.app, 'outbound-lead-generation');
  assert.equal(rec.updated_at_ms, 4242);
  // Rust order: registry target first, then the business record.
  assert.deepEqual(t.calls.writes, [
    { op: 'put', collection: TARGETS, id: 'outbound-lead-generation-policy' },
    { op: 'put', collection: 'outbound_research_policies', id: 'policy:outbound-lead-generation' },
  ]);
});

test('app falls back module -> app_id -> default; slug maps code points', async () => {
  assert.equal(appSlug('Mein App_ä'), 'mein-app--');
  const t = createTestCtx({ caps });
  const r1 = await handle(cmd('outbound.research_policy.publish', { app: '  ', module: ' Outbound ' }), t.ctx);
  assert.equal(r1.app, 'Outbound');
  assert.equal(r1.target_key, 'outbound-policy');
  const r2 = await handle(cmd('outbound.research_policy.publish', {}, { app_id: 'outbound' }), t.ctx);
  assert.equal(r2.record_id, 'policy:outbound');
  const r3 = await handle(cmd('outbound.research_policy.publish', 'not-an-object', { app_id: '   ' }), t.ctx);
  assert.equal(r3.app, 'outbound-lead-generation');
  assert.deepEqual(Object.keys(t.doc('outbound_research_policies', 'policy:outbound-lead-generation')).sort(),
    ['app', 'id', 'policy_contract', 'published_at_ms', 'updated_at_ms']);
});

test('republish keeps created_at and the active script (ON CONFLICT, registry.rs:180-195)', async () => {
  const t = createTestCtx({ now: 1000, caps });
  await handle(cmd('outbound.research_policy.publish', { app: 'x' }), t.ctx);
  await t.ctx.patch(TARGETS, 'x-policy', { latest_script_revision_no: 2, latest_script_sha256: 'abc' });
  t.setNow(2000);
  await handle(cmd('outbound.research_policy.publish', { app: 'x', fields: ['a'] }), t.ctx);
  const doc = t.doc(TARGETS, 'x-policy');
  assert.equal(doc.created_at, new Date(1000).toISOString());
  assert.equal(doc.updated_at, new Date(2000).toISOString());
  assert.equal(doc.latest_script_revision_no, 2);
  assert.equal(doc.latest_script_sha256, 'abc');
  assert.deepEqual(doc.config.fields, ['a']);
});
