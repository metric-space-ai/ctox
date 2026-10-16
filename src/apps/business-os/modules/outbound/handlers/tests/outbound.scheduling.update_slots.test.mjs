import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.scheduling.update_slots.js';

test('updates only slot keys present in payload', async () => {
  const t = createTestCtx({ data: { outbound_meeting_requests: { mr: { id: 'mr', proposed_slots: [1], duration_minutes: 30, status: 'prepared' } } } });
  await handle(cmd('outbound.scheduling.update_slots', { meeting_request_id: 'mr', proposed_slots: [], status: 'x' }), t.ctx);
  const doc = t.doc('outbound_meeting_requests', 'mr');
  assert.deepEqual(doc.proposed_slots, []);
  assert.equal(doc.duration_minutes, 30);
  assert.equal(doc.status, 'prepared');
  assert.equal((await rejects(handle(cmd('outbound.scheduling.update_slots', { meeting_request_id: 'no' }), t.ctx))).code, 'not_found');
});
