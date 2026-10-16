import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, auditCaps } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.message.reject.js';

test('rejects: message blocked, engagement draft_rejected, fallback id', async () => {
  const t = createTestCtx({ caps: auditCaps(), data: {
    outbound_messages: { msg_1: message({ revision_id: 'rev_x' }) }, outbound_engagements: { eng_1: engagement() },
  } });
  const command = cmd('outbound.message.reject', { message_id: 'msg_1', comment: 'nein' });
  delete command.id;
  await handle(command, t.ctx);
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.approval_status, 'rejected');
  assert.equal(doc.send_status, 'blocked');
  const approval = t.doc('outbound_approvals', 'rejection_msg_1_rev_x');
  assert.equal(approval.decision, 'rejected');
  assert.equal(approval.comment, 'nein');
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'draft_rejected');
});
