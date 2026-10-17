import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, auditCaps } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.message.request_changes.js';
import { stableDigest } from '../_lib/outbound.js';

test('request changes: statuses and change_request id', async () => {
  const t = createTestCtx({ caps: auditCaps(), data: {
    outbound_messages: { msg_1: message() }, outbound_engagements: { eng_1: engagement() },
  } });
  await handle(cmd('outbound.message.request_changes', { message_id: 'msg_1' }, { id: 'k' }), t.ctx);
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.approval_status, 'changes_requested');
  assert.equal(doc.draft_status, 'changes_requested');
  assert.equal(doc.send_status, 'blocked');
  assert.equal(t.doc('outbound_approvals', `change_request_${stableDigest('k')}`).decision, 'changes_requested');
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'draft_changes_requested');
});
