import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { engagement } from './_fixtures.mjs';
import handle from '../outbound.engagement.assign_sender.js';
import { stableDigest } from '../_lib/outbound.js';

test('assigns sender and writes assignment', async () => {
  const t = createTestCtx({ now: 10, data: { outbound_engagements: { eng_1: engagement({ status: 'ready_for_assignment' }) } } });
  const command = cmd('outbound.engagement.assign_sender', { engagement_id: 'eng_1', sender_account_id: 'acc_2' }, { id: 'c9' });
  const result = await handle(command, t.ctx);
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'assigned');
  assert.equal(t.doc('outbound_engagements', 'eng_1').sender_account_id, 'acc_2');
  const assignment = t.doc('outbound_sender_assignments', `assign_${stableDigest('c9')}`);
  assert.equal(assignment.status, 'active');
  assert.equal(assignment.engagement_id, 'eng_1');
  assert.equal(result.assignment.id, assignment.id);
});

test('fallback assignment id without any command id', async () => {
  const t = createTestCtx({ data: { outbound_engagements: { eng_1: engagement() } } });
  const command = cmd('outbound.engagement.assign_sender', { engagement_id: 'eng_1', sender_account_id: 'acc' });
  delete command.id;
  await handle(command, t.ctx);
  assert.ok(t.doc('outbound_sender_assignments', 'assign_eng_1_acc'));
});

test('missing engagement -> not_found, nothing written', async () => {
  const t = createTestCtx();
  const error = await rejects(handle(cmd('outbound.engagement.assign_sender', { engagement_id: 'x', sender_account_id: 'a' }), t.ctx));
  assert.equal(error.code, 'not_found');
  assert.equal(t.all('outbound_sender_assignments').length, 0);
});
