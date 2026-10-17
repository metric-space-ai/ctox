import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { engagement } from './_fixtures.mjs';
import handle from '../outbound.scheduling.mark_booked.js';

test('marks booked and returns engagement', async () => {
  const t = createTestCtx({ data: {
    outbound_engagements: { eng_1: engagement() },
    outbound_meeting_requests: { mr: { id: 'mr', engagement_id: 'eng_1', status: 'prepared', payload: {} } },
  } });
  const result = await handle(cmd('outbound.scheduling.mark_booked', { meeting_request_id: 'mr', meeting_url: 'https://m', ignored: 1 }), t.ctx);
  const doc = t.doc('outbound_meeting_requests', 'mr');
  assert.equal(doc.status, 'booked');
  assert.equal(doc.meeting_url, 'https://m');
  assert.equal(doc.ignored, undefined);
  assert.equal(result.engagement.status, 'meeting_booked');
});

test('engagement referenced but missing -> not_found and rollback', async () => {
  const t = createTestCtx({ data: { outbound_meeting_requests: { mr: { id: 'mr', engagement_id: 'gone', status: 'prepared' } } } });
  assert.equal((await rejects(handle(cmd('outbound.scheduling.mark_booked', { id: 'mr' }), t.ctx))).code, 'not_found');
  assert.equal(t.doc('outbound_meeting_requests', 'mr').status, 'prepared');
});
