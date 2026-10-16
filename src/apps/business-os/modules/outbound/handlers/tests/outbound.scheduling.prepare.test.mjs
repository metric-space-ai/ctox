import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import { engagement } from './_fixtures.mjs';
import handle from '../outbound.scheduling.prepare.js';

test('prepares meeting request, engagement -> scheduling', async () => {
  const t = createTestCtx({ data: { outbound_engagements: { eng_1: engagement() } } });
  await handle(cmd('outbound.scheduling.prepare', { meeting_request_id: 'mr', engagement_id: 'eng_1', duration_minutes: 45 }), t.ctx);
  assert.equal(t.doc('outbound_meeting_requests', 'mr').status, 'prepared');
  assert.equal(t.doc('outbound_meeting_requests', 'mr').duration_minutes, 45);
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'scheduling');
});
