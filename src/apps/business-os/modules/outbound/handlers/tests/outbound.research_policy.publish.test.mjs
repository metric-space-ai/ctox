// Strang OUT3: Ziel-Manifest über den OLG-Befehl outbound.scrape.target.upsert statt Core-Fähigkeit scrape.target.upsert.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.research_policy.publish.js';

const accept = ({ id }) => ({ id, status: 'accepted' });

test('publishes policy target via OLG command and the record', async () => {
  const t = createTestCtx({ now: 21, commandResult: accept });
  const result = await handle(cmd('outbound.research_policy.publish', { procedure: 'Schritt 1' }, { id: 'cmd-p', app_id: 'outbound-lead-generation' }), t.ctx);
  assert.equal(result.target_key, 'outbound-lead-generation-policy');
  assert.equal(result.record_id, 'policy:outbound-lead-generation');
  assert.equal(result.manifest_path, 'outbound_lead_generation_scrape_targets/outbound-lead-generation-policy');
  const [sent] = t.calls.commands;
  assert.equal(sent.type, 'outbound.scrape.target.upsert');
  assert.deepEqual(sent.options, { id: 'cmd-p:scrape-target', module: 'outbound-lead-generation' });
  assert.equal(sent.payload.manifest.target_kind, 'app-policy');
  assert.equal(sent.payload.manifest.config.policy_contract, 'ctox.outbound.research_policy.v1');
  const record = t.doc('outbound_research_policies', 'policy:outbound-lead-generation');
  assert.equal(record.procedure, 'Schritt 1');
  assert.equal(record.published_at_ms, 21);
  assert.equal(t.calls.caps.length, 0);
});

test('slug replaces non [A-Za-z0-9-] chars', async () => {
  const t = createTestCtx({ commandResult: accept });
  const result = await handle(cmd('outbound.research_policy.publish', { app: 'My App_1' }), t.ctx);
  assert.equal(result.target_key, 'my-app-1-policy');
});

test('OLG rejects → unavailable, no policy record (Rust order: manifest before record)', async () => {
  const t = createTestCtx({ commandResult: ({ id }) => ({ id, status: 'rejected', reason: 'forbidden' }) });
  const error = await rejects(handle(cmd('outbound.research_policy.publish', { app: 'x' }), t.ctx));
  assert.equal(error.code, 'unavailable');
  assert.equal(t.doc('outbound_research_policies', 'policy:x'), null);
});
