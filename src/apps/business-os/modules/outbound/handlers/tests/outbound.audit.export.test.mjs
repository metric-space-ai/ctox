import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import handle from '../outbound.audit.export.js';

test('filters by campaign id, payload.campaign_id or own id', async () => {
  const t = createTestCtx({ now: 99, data: {
    outbound_campaigns: { c1: { id: 'c1' }, c2: { id: 'c2' } },
    outbound_messages: { a: { id: 'a', campaign_id: 'c1' }, b: { id: 'b', payload: { campaign_id: 'c1' } }, c: { id: 'c', campaign_id: 'c2' } },
  } });
  const result = await handle(cmd('outbound.audit.export', { campaign_id: 'c1' }), t.ctx);
  assert.deepEqual(result.export.outbound_campaigns.map((r) => r.id), ['c1']);
  assert.deepEqual(result.export.outbound_messages.map((r) => r.id).sort(), ['a', 'b']);
  assert.equal(Object.keys(result.export).length, 11);
  assert.equal(result.exported_at_ms, 99);
  const all = await handle(cmd('outbound.audit.export', {}), t.ctx);
  assert.equal(all.export.outbound_messages.length, 3);
});
