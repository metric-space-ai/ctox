import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import { engagement, message } from './_fixtures.mjs';
import handle from '../outbound.scheduler.tick.js';

const caps = { 'mail.delivery_log.query': async () => ({ deliveries: [] }) };

test('prepares due follow-up, skips paused campaign and halted engagements', async () => {
  const t = createTestCtx({ now: 1000, caps, data: {
    outbound_campaigns: { camp_1: { id: 'camp_1', status: 'active' }, camp_p: { id: 'camp_p', status: 'paused' } },
    outbound_engagements: {
      due: engagement({ id: 'due', next_action_at_ms: 900, payload: { contact_email: 'x@y.de', sequence_id: 'seq', sequence_snapshot: { updated_at_ms: 77 } } }),
      future: engagement({ id: 'future', next_action_at_ms: 2000 }),
      paused_camp: engagement({ id: 'paused_camp', campaign_id: 'camp_p', next_action_at_ms: 1 }),
      closed: engagement({ id: 'closed', status: 'closed', next_action_at_ms: 1 }),
      ooo: engagement({ id: 'ooo', status: 'reply_received', payload: { reply_classification: 'out_of_office', next_action_at_ms: 10, contact_email: 'o@y.de' } }),
    },
    outbound_messages: { prev: message({ id: 'prev', engagement_id: 'due', subject: 'Erst', approval_status: 'approved', send_status: 'sent' }) },
  } });
  const result = await handle(cmd('outbound.scheduler.tick', {}), t.ctx);
  const kinds = Object.fromEntries(result.actions.map((a) => [a.engagement_id, a.kind]));
  assert.deepEqual(kinds, { due: 'followup_draft_prepared', paused_camp: 'followup_skipped_campaign_paused', ooo: 'followup_draft_prepared' });
  const prepared = result.actions.find((a) => a.engagement_id === 'due');
  const msg = t.doc('outbound_messages', prepared.message_id);
  assert.match(prepared.message_id, /^msg_[0-9a-f]{32}$/);
  assert.equal(msg.subject, 'Re: Erst');
  assert.equal(msg.payload.draft_engine, 'business-os.outbound.scheduler.v1');
  assert.equal(msg.payload.sequence_id, 'seq');
  assert.equal(msg.payload.sequence_version, 77);
  assert.equal(msg.reply_to_message_id, 'prev');
  const eng = t.doc('outbound_engagements', 'due');
  assert.equal(eng.status, 'awaiting_approval');
  assert.equal(eng.next_action_at_ms, 0);
  assert.equal(eng.payload.scheduler_last_message_id, prepared.message_id);
});

test('dry run reports due without writing; existing draft and account limit skip', async () => {
  const data = {
    outbound_engagements: {
      a: engagement({ id: 'a', next_action_at_ms: 1 }),
      b: engagement({ id: 'b', next_action_at_ms: 1, sender_account_id: 'Full@Mail.de' }),
    },
    outbound_messages: { m: message({ id: 'm', engagement_id: 'a', message_type: 'followup_1' }) },
    outbound_account_limits: { 'email:full@mail.de': { id: 'email:full@mail.de', daily_limit: 2, daily_sent_count: 2 } },
  };
  const dry = createTestCtx({ now: 5, caps, data });
  const dryResult = await handle(cmd('outbound.scheduler.tick', { dry_run: true }), dry.ctx);
  assert.equal(dryResult.actions.filter((a) => a.kind === 'followup_due').length, 2);
  assert.equal(dry.calls.writes.length, 0);
  const t = createTestCtx({ now: 5, caps, data });
  const result = await handle(cmd('outbound.scheduler.tick', {}), t.ctx);
  const kinds = Object.fromEntries(result.actions.map((a) => [a.engagement_id, a.kind]));
  assert.deepEqual(kinds, { a: 'followup_skipped_existing_draft', b: 'followup_skipped_account_limit' });
  assert.equal(t.doc('outbound_engagements', 'a').next_action_at_ms, 0);
  assert.equal(t.doc('outbound_engagements', 'b').payload.scheduler_last_skip_reason, 'account_limit');
  assert.equal(t.doc('outbound_engagements', 'b').next_action_at_ms, 1);
});

test('failure is recorded per engagement and rolled back', async () => {
  const t = createTestCtx({ now: 5, caps, data: { outbound_engagements: { a: engagement({ id: 'a', next_action_at_ms: 1, sender_account_id: '' }) } } });
  const result = await handle(cmd('outbound.scheduler.tick', {}), t.ctx);
  assert.equal(result.actions[0].kind, 'followup_prepare_failed');
  assert.match(result.actions[0].error, /sender_account_id is required/);
});
