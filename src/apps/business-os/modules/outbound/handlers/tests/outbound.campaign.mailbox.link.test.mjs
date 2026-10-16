import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.campaign.mailbox.link.js';

test('links mailbox: mail.channel.mailbox.link, campaign keys, default limit', async () => {
  const accounts = [];
  const t = createTestCtx({ now: 3, caps: { 'mail.channel.mailbox.link': async (a) => { accounts.push(a); return { ok: true }; } } });
  const result = await handle(cmd('outbound.campaign.mailbox.link', { campaign_id: 'c1', mailbox_address: ' Sales@Firma.de ' }), t.ctx);
  assert.equal(result.communication_account_key, 'email:sales@firma.de');
  assert.equal(accounts[0].profile.reply_to, 'Sales@Firma.de');
  const campaign = t.doc('outbound_campaigns', 'c1');
  assert.equal(campaign.mailbox_status, 'ready');
  assert.equal(campaign.payload.communication_account_address, 'Sales@Firma.de');
  const limit = t.doc('outbound_account_limits', 'email:sales@firma.de');
  assert.deepEqual([limit.daily_limit, limit.status, limit.blocked, limit.campaign_id], [0, 'active', false, 'c1']);
});

test('invalid address; capability failure aborts before writes', async () => {
  const t = createTestCtx();
  assert.equal((await rejects(handle(cmd('outbound.campaign.mailbox.link', { campaign_id: 'c1', mailbox_address: 'nope' }), t.ctx))).code, 'invalid');
  assert.equal((await rejects(handle(cmd('outbound.campaign.mailbox.link', { campaign_id: 'c1', mailbox_address: 'a@b.de' }), t.ctx))).code, 'unavailable');
  assert.equal(t.calls.writes.length, 0);
});
