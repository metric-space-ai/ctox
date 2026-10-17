import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { message } from './_fixtures.mjs';
import handle from '../outbound.message.update_draft.js';
import { messageRevision } from '../_lib/outbound.js';

test('merges only editable fields and resets statuses', async () => {
  const t = createTestCtx({ now: 30, data: { outbound_messages: { msg_1: message({ approval_status: 'approved' }) } } });
  await handle(cmd('outbound.message.update_draft', { message_id: 'msg_1', subject: 'Neu', campaign_id: 'hijack' }), t.ctx);
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.subject, 'Neu');
  assert.equal(doc.campaign_id, 'camp_1');
  assert.equal(doc.approval_status, 'draft');
  assert.equal(doc.draft_status, 'prepared');
  assert.equal(doc.send_status, 'not_scheduled');
  assert.equal(doc.revision_id, messageRevision(doc));
});

for (const status of ['queued_for_provider', 'sent', 'accepted']) {
  test(`refuses edit when ${status}`, async () => {
    const t = createTestCtx({ data: { outbound_messages: { msg_1: message({ send_status: status }) } } });
    const error = await rejects(handle(cmd('outbound.message.update_draft', { message_id: 'msg_1', subject: 'x' }), t.ctx));
    assert.equal(error.code, 'conflict');
  });
}
