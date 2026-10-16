import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { engagement, message } from './_fixtures.mjs';
import handle from '../outbound.draft.prepare.js';
import { messageRevision } from '../_lib/outbound.js';

test('followup draft with skillbook guidance and reply_to', async () => {
  const t = createTestCtx({ now: 60, data: {
    outbound_engagements: { eng_1: engagement() },
    outbound_messages: { old: message({ id: 'old', subject: 'Erstkontakt', updated_at_ms: 9 }) },
    outbound_skillbooks: { 'business-os.outbound.message_drafting.v1': { id: 'business-os.outbound.message_drafting.v1', mission: 'Mission.', non_negotiable_rules: ['Keine Floskeln'] } },
  } });
  const result = await handle(cmd('outbound.draft.prepare', { message_id: 'm2', engagement_id: 'eng_1' }), t.ctx);
  const doc = t.doc('outbound_messages', 'm2');
  assert.equal(doc.subject, 'Re: Erstkontakt');
  assert.match(doc.body_text, /^Frau Muster,\n\nich wollte meine vorherige Nachricht/);
  assert.match(doc.body_text, /Mission\. Leitplanke: Keine Floskeln\./);
  assert.equal(doc.message_type, 'followup');
  assert.equal(doc.reply_to_message_id, 'old');
  assert.equal(doc.approval_status, 'awaiting_approval');
  assert.equal(doc.payload.draft_engine, 'business-os.outbound.draft_automation.v1');
  assert.equal(doc.payload.runbook_id, '');
  assert.equal(doc.revision_id, messageRevision(doc));
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'awaiting_approval');
  assert.equal(result.provider_send_executed, false);
});

test('scheduling draft writes meeting request', async () => {
  const t = createTestCtx({ data: { outbound_engagements: { eng_1: engagement() } } });
  const result = await handle(cmd('outbound.draft.prepare', { message_id: 'm3', engagement_id: 'eng_1', draft_kind: 'scheduling', duration_minutes: 20, calendar_account_id: 'cal' }), t.ctx);
  assert.equal(result.message.subject, 'Terminvorschlag');
  assert.match(result.message.body_text, /20 Minuten/);
  const request = t.doc('outbound_meeting_requests', 'meeting_m3');
  assert.equal(request.duration_minutes, 20);
  assert.equal(request.calendar_account_id, 'cal');
  assert.deepEqual(request.proposed_slots, []);
  assert.equal(t.doc('outbound_messages', 'm3').payload.meeting_request_id, 'meeting_m3');
});

test('physical letter needs address, not email', async () => {
  const t = createTestCtx({ data: { outbound_engagements: { eng_1: engagement({ sender_account_id: undefined, payload: { channel: 'physical_letter', contact_address_text: 'Str. 1' } }) } } });
  await handle(cmd('outbound.draft.prepare', { message_id: 'm4', engagement_id: 'eng_1', draft_kind: 'initial' }), t.ctx);
  const doc = t.doc('outbound_messages', 'm4');
  assert.equal(doc.recipient_address_text, 'Str. 1');
  assert.equal(doc.sender_account_id, '');
  assert.equal(doc.subject, 'Austausch zu Ihr Unternehmen');
});

test('guards: closed engagement, suppressed recipient, missing sender', async () => {
  const closed = createTestCtx({ data: { outbound_engagements: { eng_1: engagement({ status: 'meeting_booked' }) } } });
  assert.equal((await rejects(handle(cmd('outbound.draft.prepare', { engagement_id: 'eng_1' }), closed.ctx))).code, 'conflict');
  const suppressed = createTestCtx({ data: {
    outbound_engagements: { eng_1: engagement() },
    outbound_suppression_entries: { s: { id: 's', email: 'LEAD@acme.test' } },
  } });
  assert.equal((await rejects(handle(cmd('outbound.draft.prepare', { engagement_id: 'eng_1' }), suppressed.ctx))).code, 'conflict');
  const nosender = createTestCtx({ data: { outbound_engagements: { eng_1: engagement({ sender_account_id: '' }) } } });
  assert.equal((await rejects(handle(cmd('outbound.draft.prepare', { engagement_id: 'eng_1' }), nosender.ctx))).code, 'invalid');
});
