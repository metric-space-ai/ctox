import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects, auditCaps } from './_ctx.mjs';
import handle from '../outbound.message.compose.js';
import createEngagement from '../outbound.engagement.create.js';
import prepareMessage from '../outbound.message.prepare.js';
import requestApproval from '../outbound.message.request_approval.js';
import approveMessage from '../outbound.message.approve.js';
import sendApproved from '../outbound.message.send_approved.js';

const ACCOUNT = 'email:sender@example.com';
const CONTEXT = { source_module: 'mail', target_module: 'outbound', inbound_channel: 'business_os.mail' };
const BASE = {
  campaign_id: 'camp_1',
  sender_account_id: ACCOUNT,
  subject: 'Ihr Angebot',
  body_text: 'Anbei das Angebot.',
  body_html: '<div>Anbei das Angebot.</div>',
  content_revision_id: 'rev_c1',
  created_at_ms: 77,
  client_context: CONTEXT,
};
const RECIPIENTS = [
  { recipient_email: 'a@kunde.test', engagement_id: 'eng_a', message_id: 'msg_a' },
  { recipient_email: 'b@kunde.test', engagement_id: 'eng_b', message_id: 'msg_b' },
];

// Die Einzelkette, wörtlich so, wie die mail-UI sie bis MAIL2 absetzte (index.js buildComposeCommandBundle +
// dispatchCommand: Payload + record_id + client_context; je Schritt ein eigener Befehl).
async function legacyChain(ctx, mode) {
  for (const r of RECIPIENTS) {
    const record = (id, body) => ({ ...body, record_id: id, client_context: CONTEXT });
    await createEngagement(cmd('outbound.engagement.create', record(r.engagement_id, {
      engagement_id: r.engagement_id, campaign_id: 'camp_1', sender_account_id: ACCOUNT, contact_email: r.recipient_email,
      status: 'assigned', payload: { source: 'mail_app', recipient_email: r.recipient_email }, created_at_ms: 77,
    }), { id: `legacy_e_${r.message_id}` }), ctx);
    await prepareMessage(cmd('outbound.message.prepare', record(r.message_id, {
      message_id: r.message_id, engagement_id: r.engagement_id, campaign_id: 'camp_1', channel: 'email', message_type: 'initial',
      direction: 'outbound', sender_account_id: ACCOUNT, communication_account_key: ACCOUNT, recipient_email: r.recipient_email,
      subject: 'Ihr Angebot', body_text: 'Anbei das Angebot.', body_html: '<div>Anbei das Angebot.</div>',
      payload: { source: 'mail_app', sender_address: 'sender@example.com', mail_content_revision_id: 'rev_c1', tracking_enabled: true },
      created_at_ms: 77,
    }), { id: `legacy_m_${r.message_id}` }), ctx);
    if (mode === 'approval' || mode === 'send') {
      await requestApproval(cmd('outbound.message.request_approval', record(r.message_id, { message_id: r.message_id }), { id: `legacy_r_${r.message_id}` }), ctx);
    }
    if (mode === 'send') {
      await approveMessage(cmd('outbound.message.approve', record(r.message_id, { message_id: r.message_id }), { id: `legacy_a_${r.message_id}` }), ctx);
      await sendApproved(cmd('outbound.message.send_approved', record(r.message_id, { message_id: r.message_id }), { id: `legacy_s_${r.message_id}` }), ctx);
    }
  }
}

const COLLECTIONS = ['outbound_engagements', 'outbound_messages', 'outbound_approvals', 'outbound_account_limits'];
const state = (t) => Object.fromEntries(COLLECTIONS.map((c) => [c, t.all(c).sort((a, b) => a.id.localeCompare(b.id))]));

function sendCaps(log) {
  return auditCaps({
    'mail.send': async (args) => { log.push(['mail.send', args.to[0]]); return { message_id: `q-${args.idempotency_key.split(':')[0]}`, status: 'queued', tracking: { enabled: true } }; },
  });
}
const limits = () => ({ outbound_account_limits: { [ACCOUNT]: { id: ACCOUNT, daily_limit: 10, sent_today: 0 } } });

for (const mode of ['draft', 'approval', 'send']) {
  test(`mode ${mode}: Ergebnis identisch zur Einzelkette`, async () => {
    const legacyLog = [];
    const composeLog = [];
    const legacy = createTestCtx({ now: 900, caps: sendCaps(legacyLog), data: limits() });
    const composed = createTestCtx({ now: 900, caps: sendCaps(composeLog), data: limits() });
    await legacyChain(legacy.ctx, mode);
    const result = await handle(cmd('outbound.message.compose', { ...BASE, mode, recipients: RECIPIENTS }, { id: 'cmd_compose_1' }), composed.ctx);
    assert.deepEqual(state(composed), state(legacy));
    assert.equal(result.ok, true);
    assert.deepEqual(result.items.map((i) => i.message_id), ['msg_a', 'msg_b']);
    assert.deepEqual(composeLog, legacyLog);
    const expectedStatus = { draft: 'draft_prepared', approval: 'awaiting_approval', send: 'scheduled_to_send' }[mode];
    assert.deepEqual(composed.all('outbound_engagements').map((e) => e.status), [expectedStatus, expectedStatus]);
    const audits = (t) => t.calls.caps.filter((c) => c.name === 'audit.event').map((c) => c.args[0].record_id);
    assert.deepEqual(audits(composed), audits(legacy));
    if (mode === 'send') assert.equal(composed.doc('outbound_messages', 'msg_a').send_status, 'queued_for_provider');
  });
}

test('alle Schreibungen in einer Transaktion: Fehler mitten in der Kette hinterlässt nichts', async () => {
  const t = createTestCtx({ now: 5, caps: auditCaps() });
  // Engagement + Nachricht sind im tx schon geschrieben, dann wirft request_approval invalid (Freigabe ohne
  // Inhalt – Regel des Einzel-Handlers, nicht neu) → Rollback.
  const error = await rejects(handle(cmd('outbound.message.compose', {
    ...BASE, body_text: undefined, body_html: undefined, mode: 'approval', recipients: RECIPIENTS,
  }), t.ctx));
  assert.equal(error.code, 'invalid');
  assert.deepEqual(t.all('outbound_engagements'), []);
  assert.deepEqual(t.all('outbound_messages'), []);
  assert.equal(t.calls.caps.length, 0);
});

test('Audit-Ereignis der Freigabe erst nach dem Commit', async () => {
  let seen = null;
  const t = createTestCtx({ now: 5, caps: {
    'audit.event': async (args) => { seen = t.doc('outbound_approvals', args.record_id); return { ok: true }; },
    'mail.send': async (args) => ({ message_id: `q-${args.idempotency_key.split(':')[0]}`, status: 'queued' }),
  }, data: limits() });
  await handle(cmd('outbound.message.compose', { ...BASE, mode: 'send', recipients: [RECIPIENTS[0]] }), t.ctx);
  assert.equal(seen?.decision, 'approved', 'Freigabe ist beim Audit-Aufruf bereits committet');
});

test('Versandfehler: Entwürfe/Freigaben bleiben, Fehler wird wie bei send_approved gebucht und geworfen', async () => {
  const t = createTestCtx({ now: 5, caps: auditCaps({
    'mail.send': async () => { throw Object.assign(new Error('smtp down'), { code: 'unavailable' }); },
  }), data: limits() });
  const error = await rejects(handle(cmd('outbound.message.compose', { ...BASE, mode: 'send', recipients: RECIPIENTS }), t.ctx));
  assert.equal(error.code, 'unavailable');
  assert.match(error.message, /2 of 2/);
  assert.equal(t.all('outbound_approvals').length, 2);
  assert.equal(t.doc('outbound_messages', 'msg_a').approval_status, 'approved');
  assert.equal(t.doc('outbound_messages', 'msg_a').payload.last_send_error, 'failed to queue approved outbound email');
});

test('Eingabeprüfung: Empfänger und Modus', async () => {
  const t = createTestCtx();
  assert.equal((await rejects(handle(cmd('outbound.message.compose', { ...BASE, recipients: [] }), t.ctx))).code, 'invalid');
  assert.equal((await rejects(handle(cmd('outbound.message.compose', { ...BASE, mode: 'x', recipients: RECIPIENTS }), t.ctx))).code, 'invalid');
  assert.equal((await rejects(handle(cmd('outbound.message.compose', { ...BASE, recipients: [{}] }), t.ctx))).code, 'invalid');
  // campaign_id fehlt → Regel von outbound.engagement.create
  assert.equal((await rejects(handle(cmd('outbound.message.compose', { ...BASE, campaign_id: undefined, recipients: RECIPIENTS }), t.ctx))).code, 'invalid');
});

test('ohne Ids: deterministisch aus der Befehls-Id, Empfänger als Zeichenketten', async () => {
  const t = createTestCtx({ now: 5 });
  const result = await handle(cmd('outbound.message.compose', { ...BASE, recipients: ['x@kunde.test'] }, { id: 'c9' }), t.ctx);
  const [item] = result.items;
  assert.match(item.engagement_id, /^eng_[0-9a-f]+$/);
  assert.equal(t.doc('outbound_messages', item.message_id).recipient_email, 'x@kunde.test');
});
