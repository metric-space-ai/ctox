import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.engagement.resume.js';

test('status from paused message', async () => {
  const t = createTestCtx({ data: {
    outbound_engagements: { eng_1: engagement({ status: 'paused', paused_reason: 'x' }) },
    outbound_messages: { msg_1: message({ send_status: 'paused', approval_status: 'awaiting_approval' }) },
  } });
  await handle(cmd('outbound.engagement.resume', { engagement_id: 'eng_1' }), t.ctx);
  const eng = t.doc('outbound_engagements', 'eng_1');
  assert.equal(eng.status, 'awaiting_approval');
  assert.equal(eng.paused_reason, '');
  assert.equal(eng.payload.resume_reason, 'manual_resume');
});

test('no paused message -> assigned; non-paused engagement refused', async () => {
  const t = createTestCtx({ data: { outbound_engagements: { eng_1: engagement({ status: 'paused' }), eng_2: engagement({ id: 'eng_2' }) } } });
  await handle(cmd('outbound.engagement.resume', { id: 'eng_1' }), t.ctx);
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'assigned');
  assert.equal((await rejects(handle(cmd('outbound.engagement.resume', { id: 'eng_2' }), t.ctx))).code, 'conflict');
});
