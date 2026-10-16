import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects, auditCaps } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.message.approve.js';
import { messageRevision, stableDigest } from '../_lib/outbound.js';

test('approves, writes approval, engagement approved_for_send, audit event', async () => {
  const t = createTestCtx({ now: 40, caps: auditCaps(), data: {
    outbound_messages: { msg_1: message() }, outbound_engagements: { eng_1: engagement() },
  } });
  const result = await handle(cmd('outbound.message.approve', { message_id: 'msg_1' }, { id: 'c1', actor: { id: 'u7', role: 'admin' } }), t.ctx);
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.approval_status, 'approved');
  assert.equal(doc.send_status, 'approved_not_sent');
  assert.equal(doc.revision_id, messageRevision(message()));
  const approval = t.doc('outbound_approvals', `approval_${stableDigest('c1')}`);
  assert.equal(approval.decision, 'approved');
  assert.equal(approval.actor_user_id, 'u7');
  assert.equal(approval.revision_id, doc.revision_id);
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'approved_for_send');
  const event = t.calls.caps.find((c) => c.name === 'audit.event');
  assert.equal(event.args[0].event_type, 'business_os.external_approval.decided');
  assert.equal(event.args[0].payload.decision, 'approved');
  assert.equal(result.approval.id, approval.id);
});

test('only awaiting_approval can be approved', async () => {
  const t = createTestCtx({ caps: auditCaps(), data: { outbound_messages: { msg_1: message({ approval_status: 'draft' }) } } });
  const error = await rejects(handle(cmd('outbound.message.approve', { message_id: 'msg_1' }), t.ctx));
  assert.equal(error.code, 'conflict');
  assert.equal(t.all('outbound_approvals').length, 0);
});

test('revision matches Rust serde preserve_order material', () => {
  // material = {"sender_account_id":"a","recipient_email":"b","subject":"c","body_text":"d","body_html":""}
  const expected = `rev_${stableDigest('{"sender_account_id":"a","recipient_email":"b","subject":"c","body_text":"d","body_html":""}')}`;
  assert.equal(messageRevision({ sender_account_id: 'a', recipient_email: ' b ', subject: 'c', body_text: 'd' }), expected);
});
