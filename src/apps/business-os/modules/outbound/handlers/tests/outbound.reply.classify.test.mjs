import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import { engagement } from './_fixtures.mjs';
import handle from '../outbound.reply.classify.js';
import { stableDigest } from '../_lib/outbound.js';

test('unsubscribe creates suppression and stops engagement', async () => {
  const t = createTestCtx({ now: 100, data: { outbound_engagements: { eng_1: engagement({ payload: { contact_email: 'Lead@Acme.test' } }) } } });
  const result = await handle(cmd('outbound.reply.classify', { engagement_id: 'eng_1', classification: 'opt-out', reply_message_id: 'r1' }), t.ctx);
  const id = `supp_reply_${stableDigest('lead@acme.test|unsubscribe')}`;
  assert.equal(result.suppression_id, id);
  // Schema form only (Strang FIXINV, INV3-B3): address entry + domain entry, as Rust's
  // single legacy entry with email AND domain blocked both.
  const entry = t.doc('outbound_suppression_entries', id);
  assert.deepEqual(
    { scope: entry.scope, value: entry.value, active: entry.active, reason: entry.reason, email: entry.email, status: entry.status },
    { scope: 'email', value: 'lead@acme.test', active: 1, reason: 'unsubscribe', email: undefined, status: undefined },
  );
  const domainEntry = t.doc('outbound_suppression_entries', `supp_reply_domain_${stableDigest('acme.test|unsubscribe')}`);
  assert.deepEqual(
    { scope: domainEntry.scope, value: domainEntry.value, active: domainEntry.active, reason: domainEntry.reason, domain: domainEntry.domain },
    { scope: 'domain', value: 'acme.test', active: 1, reason: 'unsubscribe', domain: undefined },
  );
  assert.equal(t.all('outbound_suppression_entries').length, 2);
  const eng = t.doc('outbound_engagements', 'eng_1');
  assert.equal(eng.status, 'stopped');
  assert.equal(eng.payload.stop_reason, 'unsubscribe');
  assert.equal(eng.reply_message_id, 'r1');
  assert.equal(result.engagement.status, 'reply_received');
});

test('out_of_office schedules wait, no suppression; existing suppression idempotent', async () => {
  const t = createTestCtx({ now: 100, data: { outbound_engagements: { eng_1: engagement() } } });
  const result = await handle(cmd('outbound.reply.classify', { engagement_id: 'eng_1', classification: 'out_of_office', ooo_until: 50 }), t.ctx);
  assert.equal(result.suppression_id, null);
  const eng = t.doc('outbound_engagements', 'eng_1');
  assert.equal(eng.status, 'reply_received');
  assert.equal(eng.next_action_at_ms, 100 + 3 * 24 * 60 * 60 * 1000);
  assert.equal(eng.payload.reply_wait_reason, 'out_of_office');

  const t2 = createTestCtx({ data: {
    outbound_engagements: { eng_1: engagement() },
    outbound_suppression_entries: { s: { id: 's', domain: 'acme.test', reason: 'manual' } },
  } });
  const r2 = await handle(cmd('outbound.reply.classify', { engagement_id: 'eng_1', classification: 'bounce' }), t2.ctx);
  assert.equal(r2.suppression_id, null);
  assert.equal(t2.all('outbound_suppression_entries').length, 1);
});
