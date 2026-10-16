import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.engagement.create.js';
import { stableDigest } from '../_lib/outbound.js';

test('creates engagement with defaults', async () => {
  const t = createTestCtx({ now: 500 });
  const result = await handle(cmd('outbound.engagement.create', { engagement_id: 'eng_a', campaign_id: 'c1', extra: 1 }), t.ctx);
  assert.equal(result.ok, true);
  const doc = t.doc('outbound_engagements', 'eng_a');
  assert.equal(doc.status, 'ready_for_assignment');
  assert.deepEqual(doc.payload, {});
  assert.equal(doc.created_at_ms, 500);
  assert.equal(doc.extra, 1);
});

test('keeps given status and created_at, derives id from command id', async () => {
  const t = createTestCtx({ now: 900 });
  const command = cmd('outbound.engagement.create', { campaign_id: 'c1', status: 'x', created_at_ms: 3 }, { id: 'cmd-42' });
  await handle(command, t.ctx);
  const doc = t.doc('outbound_engagements', `eng_${stableDigest('cmd-42')}`);
  assert.equal(doc.status, 'x');
  assert.equal(doc.created_at_ms, 3);
  assert.equal(doc.updated_at_ms, 900);
});

test('campaign_id required', async () => {
  const t = createTestCtx();
  const error = await rejects(handle(cmd('outbound.engagement.create', { engagement_id: 'e' }), t.ctx));
  assert.equal(error.code, 'invalid');
  assert.match(error.message, /campaign_id is required/);
});
