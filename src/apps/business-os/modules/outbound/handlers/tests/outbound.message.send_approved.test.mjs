import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { engagement, message } from './_fixtures.mjs';
import handle from '../outbound.message.send_approved.js';
import { messageRevision } from '../_lib/outbound.js';

function approvedData(overrides = {}, extra = {}) {
  const msg = message({ approval_status: 'approved', send_status: 'approved_not_sent', ...overrides });
  msg.revision_id = messageRevision(msg);
  return {
    outbound_messages: { msg_1: msg },
    outbound_engagements: { eng_1: engagement() },
    outbound_approvals: { ap: { id: 'ap', message_id: 'msg_1', revision_id: msg.revision_id, decision: 'approved' } },
    ...extra,
  };
}

function sendCaps(sent = [], comm = []) {
  return {
    'mail.send': async (args) => { sent.push(args); return { message_id: 'q-1', status: 'queued' }; },
  };
}

test('queues email, reserves slot, syncs communication, engagement scheduled_to_send', async () => {
  const sent = [];
  const comm = [];
  const t = createTestCtx({ now: 500, caps: sendCaps(sent, comm), data: approvedData({}, {
    outbound_account_limits: { 'email:sender@example.com': { id: 'email:sender@example.com', daily_limit: 5, sent_today: 1 } },
  }) });
  const result = await handle(cmd('outbound.message.send_approved', { message_id: 'msg_1' }), t.ctx);
  assert.equal(result.provider_queue_id, 'q-1');
  assert.equal(sent[0].account_id, 'email:sender@example.com');
  assert.deepEqual(Object.keys(sent[0]).sort(), ['account_id', 'html', 'idempotency_key', 'subject', 'text', 'to']);
  assert.deepEqual(sent[0].to, ['lead@acme.test']);
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.send_status, 'queued_for_provider');
  assert.equal(doc.provider_message_id, 'q-1');
  assert.equal(doc.payload.provider_send_executed, true);
  // Nur-Text-Nachricht: Rust bereitet Tracking nur bei HTML vor (store_outbound_commands.rs:6116) – kein Stempel.
  assert.equal(doc.payload.tracking_enabled, undefined);
  assert.match(doc.communication_message_key, /^email:sender@example\.com:outbound:[0-9a-f]{24}$/);
  const row = t.doc('communication_messages', doc.communication_message_key);
  assert.equal(row.status, 'queued_for_provider');
  assert.equal(row.direction, 'outbound');
  assert.deepEqual(row.recipient_addresses_json, ['lead@acme.test']);
  assert.equal(row.metadata_json.provider_queue_id, 'q-1');
  const thread = t.doc('communication_threads', row.thread_key);
  assert.equal(thread.message_count, 1);
  assert.equal(thread.last_message_key, row.message_key);
  assert.equal(comm.length, 0);
  const limit = t.doc('outbound_account_limits', 'email:sender@example.com');
  assert.equal(limit.sent_today, 2);
  assert.equal(limit.remaining_today, 3);
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'scheduled_to_send');
});

test('gate failure records send_blocked and throws', async () => {
  const t = createTestCtx({ caps: sendCaps(), data: approvedData({}, { outbound_approvals: {} }) });
  const error = await rejects(handle(cmd('outbound.message.send_approved', { message_id: 'msg_1' }), t.ctx));
  assert.equal(error.code, 'conflict');
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.send_status, 'send_blocked');
  assert.equal(doc.payload.send_block_reason, 'approval_required');
  assert.equal(doc.payload.send_attempts, 1);
  assert.equal(doc.payload.retryable, true);
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'send_blocked');
});

test('daily limit exhausted -> quota, sender_limit_exhausted', async () => {
  const t = createTestCtx({ caps: sendCaps(), data: approvedData({}, {
    outbound_account_limits: { 'email:sender@example.com': { id: 'email:sender@example.com', daily_limit: 1, sent_today: 1 } },
  }) });
  const error = await rejects(handle(cmd('outbound.message.send_approved', { message_id: 'msg_1' }), t.ctx));
  assert.equal(error.code, 'quota');
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.send_block_reason, 'sender_limit_exhausted');
});

test('provider failure releases slot; Rust classifier quirk yields approval_required', async () => {
  const t = createTestCtx({ caps: { 'mail.send': async () => { throw Object.assign(new Error('smtp down'), { code: 'unavailable' }); } }, data: approvedData({}, {
    outbound_account_limits: { 'email:sender@example.com': { id: 'email:sender@example.com', daily_limit: 5, sent_today: 1 } },
  }) });
  const error = await rejects(handle(cmd('outbound.message.send_approved', { message_id: 'msg_1' }), t.ctx));
  assert.match(error.message, /failed to queue approved outbound email: smtp down/);
  assert.equal(t.doc('outbound_account_limits', 'email:sender@example.com').sent_today, 1);
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.send_block_reason, 'approval_required');
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.last_send_error, 'failed to queue approved outbound email');
});

test('idempotent when already queued; physical letter marked sent', async () => {
  const sent = [];
  const t = createTestCtx({ caps: sendCaps(sent), data: approvedData({ send_status: 'queued_for_provider', provider_message_id: 'q0', payload: { provider_send_executed: true } }) });
  const result = await handle(cmd('outbound.message.send_approved', { message_id: 'msg_1' }), t.ctx);
  assert.equal(result.idempotent, true);
  assert.equal(sent.length, 0);

  const letter = createTestCtx({ now: 9, data: approvedData({ channel: 'physical_letter', recipient_address_text: 'Weg 1' }) });
  const r1 = await handle(cmd('outbound.message.send_approved', { message_id: 'msg_1' }), letter.ctx);
  assert.equal(r1.provider_send_executed, true);
  assert.equal(letter.doc('outbound_messages', 'msg_1').send_status, 'sent');
  assert.equal(letter.doc('outbound_engagements', 'eng_1').status, 'sent');
  const r2 = await handle(cmd('outbound.message.send_approved', { message_id: 'msg_1' }), letter.ctx);
  assert.equal(r2.idempotent, true);
});

test('suppressed recipient blocked', async () => {
  const t = createTestCtx({ caps: sendCaps(), data: approvedData({}, { outbound_suppression_entries: { s: { id: 's', email: 'lead@acme.test', reason: 'bounce' } } }) });
  const error = await rejects(handle(cmd('outbound.message.send_approved', { message_id: 'msg_1' }), t.ctx));
  assert.match(error.message, /reason: bounce/);
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.send_block_reason, 'recipient_suppressed');
});
