// Geschäfts-Invarianten 1–4 der Outbound-App (Strang INV, Shell v4).
// Ersetzen die mit dem Rust-Code wegfallenden Wächter aus store_outbound_commands.rs.
// Je Invariante genau EIN Test; Mutationsbeleg: tests/mutation-proof.mjs, Bericht: INVARIANTS.md.
// Testnamen beginnen mit "INV<n>" – mutation-proof.mjs filtert darauf.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, auditCaps } from './_ctx.mjs';
import { engagement, message } from './_fixtures.mjs';
import send from '../outbound.message.send_approved.js';
import approve from '../outbound.message.approve.js';
import reject from '../outbound.message.reject.js';
import classifyReply from '../outbound.reply.classify.js';
import { messageRevision } from '../_lib/outbound.js';

const SENDER = 'email:sender@example.com';

/**
 * Mail capability as the contract demands it: the provider queue dedupes on
 * idempotency_key. `calls` = every invocation by a handler, `delivered` =
 * mails that actually reach the queue (one per distinct key).
 */
function mailbox({ failCommunicationOnce = false } = {}) {
  const calls = [];
  const delivered = new Map();
  let communicationFailures = failCommunicationOnce ? 1 : 0;
  return {
    calls,
    delivered,
    caps: auditCaps({
      'mail.send': async (args) => {
        calls.push(args);
        if (!delivered.has(args.idempotency_key)) {
          delivered.set(args.idempotency_key, { queue_id: `q-${delivered.size + 1}`, to: args.to, subject: args.subject, text: args.text });
        }
        return { message_id: delivered.get(args.idempotency_key).queue_id, status: 'queued' };
      },
    }),
    // Absturz zwischen Provider-Queue und Persistenz: der erste Schreibversuch der Kommunikationszeile
    // (communication_messages, App conversations) scheitert.
    inject(ctx) {
      const realPut = ctx.put;
      ctx.put = async (collection, doc) => {
        if (collection === 'communication_messages' && communicationFailures > 0) {
          communicationFailures -= 1;
          throw Object.assign(new Error('process aborted between provider queue and persistence'), { code: 'unavailable' });
        }
        return realPut(collection, doc);
      };
    },
  };
}

function awaiting(id, overrides = {}) {
  return message({ id, approval_status: 'awaiting_approval', send_status: 'awaiting_approval', ...overrides });
}

async function outcome(promise) {
  try {
    return { ok: true, value: await promise };
  } catch (error) {
    return { ok: false, code: error?.code, message: String(error?.message ?? error) };
  }
}

let seq = 0;
const run = (handler, type, payload, ctx) => outcome(handler(cmd(type, payload, { id: `cmd_${type}_${++seq}` }), ctx));

// ---------------------------------------------------------------------------
// INV1 – Versand nur bei aktueller freigegebener Revision.
// Rust: outbound_message_send_approved_requires_matching_current_revision,
//       outbound_send_approved_blocked_after_rejection_or_change_request.
test('INV1 send only for the current approved revision', async () => {
  const mail = mailbox();
  const t = createTestCtx({ caps: mail.caps, data: {
    outbound_engagements: { eng_1: engagement() },
    outbound_messages: {
      m_unapproved: awaiting('m_unapproved'),
      m_changed: awaiting('m_changed', { body_text: 'Old body' }),
      m_rejected: awaiting('m_rejected'),
      m_ok: awaiting('m_ok', { body_text: 'Approved body' }),
    },
  } });

  // (a) never approved -> no send
  const unapproved = await run(send, 'outbound.message.send_approved', { message_id: 'm_unapproved' }, t.ctx);
  assert.equal(unapproved.ok, false);
  assert.match(unapproved.message, /must be approved before send/);

  // (b) approved, then the content changed (new revision) while the status still says approved
  assert.equal((await run(approve, 'outbound.message.approve', { message_id: 'm_changed' }, t.ctx)).ok, true);
  const changed = t.doc('outbound_messages', 'm_changed');
  changed.body_text = 'Changed body';
  changed.revision_id = messageRevision(changed);
  await t.ctx.put('outbound_messages', changed);
  const stale = await run(send, 'outbound.message.send_approved', { message_id: 'm_changed' }, t.ctx);
  assert.equal(stale.ok, false);
  assert.match(stale.message, /no matching approval for current revision/);

  // (c) approved, then rejected -> the old approval row must not carry the send
  assert.equal((await run(approve, 'outbound.message.approve', { message_id: 'm_rejected' }, t.ctx)).ok, true);
  assert.equal((await run(reject, 'outbound.message.reject', { message_id: 'm_rejected' }, t.ctx)).ok, true);
  const rejected = await run(send, 'outbound.message.send_approved', { message_id: 'm_rejected' }, t.ctx);
  assert.equal(rejected.ok, false);

  // (d) INV1-B1 (Strang FIXINV): content changed after approval by a write that BYPASSES the
  // handlers (revision_id, approval_status stay as approved) -> the approval no longer covers it.
  // One message per send-relevant field: body, subject, recipient, attachments.
  const directEdits = {
    m_body: (doc) => { doc.body_text = 'Unapproved body'; },
    m_subject: (doc) => { doc.subject = 'Unapproved subject'; },
    m_recipient: (doc) => { doc.recipient_email = 'someone-else@acme.test'; },
    m_attachment: (doc) => { doc.payload.attachments = [{ name: 'unapproved.pdf', sha256: 'abc' }]; },
  };
  for (const [id, edit] of Object.entries(directEdits)) {
    await t.ctx.put('outbound_messages', awaiting(id, { body_text: 'Approved body' }));
    assert.equal((await run(approve, 'outbound.message.approve', { message_id: id }, t.ctx)).ok, true, id);
    const doc = t.doc('outbound_messages', id);
    const approvedRevision = doc.revision_id;
    edit(doc);
    await t.ctx.put('outbound_messages', doc);
    const result = await run(send, 'outbound.message.send_approved', { message_id: id }, t.ctx);
    assert.equal(result.ok, false, `${id}: content changed after approval was sent`);
    assert.equal(result.code, 'conflict', id);
    assert.match(result.message, /no matching approval for current revision/, id);
    const after = t.doc('outbound_messages', id);
    assert.equal(after.revision_id, approvedRevision, `${id}: the stored revision was left untouched`);
    assert.equal(after.payload.send_block_reason, 'approval_required', id);
  }

  // (e) Bestand: an approval written before the content hash existed (only revision_id) is
  // checked against the revision RECOMPUTED from the content, never the stored revision_id.
  const legacyRevision = messageRevision(awaiting('m_legacy_changed', { body_text: 'Approved body' }));
  for (const id of ['m_legacy_changed', 'm_legacy_ok']) {
    const legacy = awaiting(id, { body_text: 'Approved body', approval_status: 'approved', send_status: 'approved_not_sent', revision_id: legacyRevision });
    await t.ctx.put('outbound_messages', legacy);
    await t.ctx.put('outbound_approvals', { id: `approval_${id}`, message_id: id, engagement_id: 'eng_1', revision_id: legacyRevision, decision: 'approved', created_at_ms: 1, updated_at_ms: 1 });
  }
  const legacyDoc = t.doc('outbound_messages', 'm_legacy_changed');
  legacyDoc.body_text = 'Unapproved body';
  await t.ctx.put('outbound_messages', legacyDoc);
  const legacyChanged = await run(send, 'outbound.message.send_approved', { message_id: 'm_legacy_changed' }, t.ctx);
  assert.equal(legacyChanged.ok, false, 'legacy approval does not cover changed content');
  assert.match(legacyChanged.message, /no matching approval for current revision/);
  const legacyOk = await run(send, 'outbound.message.send_approved', { message_id: 'm_legacy_ok' }, t.ctx);
  assert.equal(legacyOk.ok, true, `legacy approval of unchanged content still sends: ${legacyOk.message}`);
  mail.calls.length = 0;

  // control: approved and unchanged -> exactly the approved content goes out
  assert.equal((await run(approve, 'outbound.message.approve', { message_id: 'm_ok' }, t.ctx)).ok, true);
  const ok = await run(send, 'outbound.message.send_approved', { message_id: 'm_ok' }, t.ctx);
  assert.equal(ok.ok, true, ok.message);

  assert.equal(mail.calls.length, 1, 'only the approved, unchanged message reaches mail.send');
  assert.match(mail.calls[0].idempotency_key, /^m_ok:/);
  assert.equal(mail.calls[0].text, 'Approved body');
});

// ---------------------------------------------------------------------------
// INV2 – Kein doppelter Versand, auch nicht bei Wiederholung oder Crash.
// Rust: outbound_send_approved_is_idempotent_for_already_queued_message,
//       outbound_message_send_is_idempotent_after_queueing.
test('INV2 no double send on repeat, crash between steps, or parallel duplicates', async () => {
  const approvedData = async (mail) => {
    const t = createTestCtx({ caps: mail.caps, data: {
      outbound_engagements: { eng_1: engagement() },
      outbound_messages: { m: awaiting('m') },
      outbound_account_limits: { [SENDER]: { id: SENDER, daily_limit: 50, sent_today: 0 } },
    } });
    assert.equal((await run(approve, 'outbound.message.approve', { message_id: 'm' }, t.ctx)).ok, true);
    return t;
  };

  // (a) the same send, dispatched twice as distinct command envelopes
  const repeat = mailbox();
  const r = await approvedData(repeat);
  assert.equal((await run(send, 'outbound.message.send_approved', { message_id: 'm' }, r.ctx)).ok, true);
  const second = await run(send, 'outbound.message.send_approved', { message_id: 'm' }, r.ctx);
  assert.equal(second.ok, true);
  assert.equal(second.value.idempotent, true);
  assert.equal(repeat.calls.length, 1, 'a queued message is never handed to mail.send again');
  assert.equal(r.doc('outbound_account_limits', SENDER).sent_today, 1, 'the repeat does not consume a second slot');

  // (b) abort after the provider accepted the mail but before the message was persisted, then retry
  const crash = mailbox({ failCommunicationOnce: true });
  const c = await approvedData(crash);
  crash.inject(c.ctx);
  assert.equal((await run(send, 'outbound.message.send_approved', { message_id: 'm' }, c.ctx)).ok, false);
  assert.equal((await run(send, 'outbound.message.send_approved', { message_id: 'm' }, c.ctx)).ok, true);
  assert.equal(crash.calls.length, 2, 'the retry reaches the provider again');
  assert.equal(new Set(crash.calls.map((call) => call.idempotency_key)).size, 1, 'retry reuses the idempotency key');
  assert.equal(crash.delivered.size, 1, 'exactly one mail delivered');
  assert.equal(c.doc('outbound_messages', 'm').send_status, 'queued_for_provider');

  // (c) two identical sends in parallel
  const parallel = mailbox();
  const p = await approvedData(parallel);
  await Promise.all([
    run(send, 'outbound.message.send_approved', { message_id: 'm' }, p.ctx),
    run(send, 'outbound.message.send_approved', { message_id: 'm' }, p.ctx),
  ]);
  assert.equal(parallel.delivered.size, 1, 'parallel duplicates deliver one mail');
});

// ---------------------------------------------------------------------------
// INV3 – Gesperrte bzw. abgemeldete Empfänger werden nie angeschrieben.
// Rust: outbound_message_send_blocked_when_recipient_is_suppressed,
//       outbound_unsubscribe_reply_creates_suppression_and_blocks_send,
//       outbound_send_gate_blocks_bounce_and_unhealthy_sender.
test('INV3 suppressed or unsubscribed recipients are never mailed', async () => {
  const mail = mailbox();
  const t = createTestCtx({ caps: mail.caps, data: {
    outbound_engagements: {
      eng_1: engagement(),
      eng_unsub: engagement({ id: 'eng_unsub', payload: { contact_email: 'leaver@acme.test' } }),
    },
    outbound_messages: {
      m_exact: awaiting('m_exact', { recipient_email: 'bounced@acme.test' }),
      m_domain: awaiting('m_domain', { recipient_email: 'anyone@blocked.test' }),
      m_unsub: awaiting('m_unsub', { engagement_id: 'eng_unsub', recipient_email: 'leaver@acme.test' }),
    },
    outbound_suppression_entries: {
      // legacy form (Bestand, still read)
      s_exact: { id: 's_exact', email: 'Bounced@Acme.TEST', reason: 'bounce', status: 'active' },
      s_domain: { id: 's_domain', domain: 'blocked.test', reason: 'complaint', status: 'active' },
      // schema form scope/value/active (INV3-B3, Strang FIXINV) – the only form written now
      s_schema_email: { id: 's_schema_email', scope: 'email', value: 'OptOut@Example.ORG', reason: 'unsubscribe', active: 1, created_at_ms: 1, updated_at_ms: 1 },
      s_schema_domain: { id: 's_schema_domain', scope: 'domain', value: 'schema-blocked.test', reason: 'complaint', active: 1, created_at_ms: 1, updated_at_ms: 1 },
      // inactive schema entries block nobody
      s_schema_off: { id: 's_schema_off', scope: 'email', value: 'welcome@partner.test', reason: 'manual', active: 0, created_at_ms: 1, updated_at_ms: 1 },
      s_schema_off_domain: { id: 's_schema_off_domain', scope: 'domain', value: 'reactivated.test', reason: 'manual', active: 0, created_at_ms: 1, updated_at_ms: 1 },
    },
  } });
  const blocked = {
    m_schema_email: 'optout@example.org',
    m_schema_domain: 'someone@schema-blocked.test',
    m_schema_subdomain: 'someone@eu.schema-blocked.test',
  };
  for (const [id, recipient] of Object.entries(blocked)) await t.ctx.put('outbound_messages', awaiting(id, { recipient_email: recipient }));
  await t.ctx.put('outbound_messages', awaiting('m_inactive', { recipient_email: 'welcome@partner.test' }));
  await t.ctx.put('outbound_messages', awaiting('m_inactive_domain', { recipient_email: 'x@reactivated.test' }));
  await t.ctx.put('outbound_messages', awaiting('m_lookalike', { recipient_email: 'x@notschema-blocked.test' }));
  const suppressedIds = ['m_exact', 'm_domain', 'm_unsub', ...Object.keys(blocked)];
  const clearIds = ['m_inactive', 'm_inactive_domain', 'm_lookalike'];
  for (const id of [...suppressedIds, ...clearIds]) {
    assert.equal((await run(approve, 'outbound.message.approve', { message_id: id }, t.ctx)).ok, true);
  }
  // the recipient answers "unsubscribe" after the draft was approved
  assert.equal((await run(classifyReply, 'outbound.reply.classify', { engagement_id: 'eng_unsub', classification: 'unsubscribe' }, t.ctx)).ok, true);
  // the reply path writes the schema form only
  for (const entry of t.all('outbound_suppression_entries').filter((e) => e.source === 'reply_classification')) {
    assert.ok(['email', 'domain'].includes(entry.scope) && entry.value && entry.active === 1, JSON.stringify(entry));
    assert.equal(entry.email ?? entry.domain ?? entry.status, undefined, 'no legacy fields on new entries');
  }

  // control: inactive entries and a look-alike domain do not block
  for (const id of clearIds) {
    const result = await run(send, 'outbound.message.send_approved', { message_id: id }, t.ctx);
    assert.equal(result.ok, true, `${id}: ${result.message}`);
  }
  assert.deepEqual(mail.calls.map((c) => c.to[0]), ['welcome@partner.test', 'x@reactivated.test', 'x@notschema-blocked.test']);
  mail.calls.length = 0;

  for (const id of suppressedIds) {
    const result = await run(send, 'outbound.message.send_approved', { message_id: id }, t.ctx);
    assert.equal(result.ok, false, id);
    assert.match(result.message, /recipient is suppressed/, id);
    assert.equal(t.doc('outbound_messages', id).payload.send_block_reason, 'recipient_suppressed', id);
  }
  assert.equal(mail.calls.length, 0, 'no suppressed recipient reaches mail.send');
});

// ---------------------------------------------------------------------------
// INV4 – Das Tageslimit gilt auch bei parallelen Befehlen.
// Rust: outbound_daily_limit_enforced_under_parallel_commands (dort wie hier:
// Konflikte/Sperren dürfen Versände kosten, nie das Limit überschreiten).
const N = 6;
const CAP = 3;

async function parallelBurstWithRetries() {
  const mail = mailbox();
  const messages = {};
  for (let i = 0; i < N; i += 1) messages[`m${i}`] = awaiting(`m${i}`, { recipient_email: `lead${i}@acme.test` });
  const t = createTestCtx({ caps: mail.caps, data: {
    outbound_engagements: { eng_1: engagement() },
    outbound_messages: messages,
    outbound_account_limits: { [SENDER]: { id: SENDER, daily_limit: CAP, daily_sent_count: 0, sent_today: 0 } },
  } });
  for (const id of Object.keys(messages)) {
    assert.equal((await run(approve, 'outbound.message.approve', { message_id: id }, t.ctx)).ok, true);
  }
  // burst: all N sends at once (Promise.all over the transaction semantics of the ctx)
  let pending = Object.keys(messages);
  let results = await Promise.all(pending.map((id) => run(send, 'outbound.message.send_approved', { message_id: id }, t.ctx)));
  const burstDelivered = mail.delivered.size;
  // commands that lost a transaction conflict are dispatched again (as the runtime/user would)
  const finals = new Map();
  for (let round = 0; round <= N && pending.length > 0; round += 1) {
    const retry = [];
    pending.forEach((id, index) => {
      if (results[index].ok || results[index].code !== 'conflict') finals.set(id, results[index]);
      else retry.push(id);
    });
    pending = retry;
    results = [];
    for (const id of pending) results.push(await run(send, 'outbound.message.send_approved', { message_id: id }, t.ctx));
  }
  return { mail, t, finals, burstDelivered, unresolved: pending };
}

test('INV4 daily limit holds under parallel send commands', async () => {
  const { mail, t, finals, burstDelivered, unresolved } = await parallelBurstWithRetries();
  const limit = t.doc('outbound_account_limits', SENDER);
  const ok = [...finals.values()].filter((r) => r.ok).length;
  const quota = [...finals.values()].filter((r) => !r.ok && r.code === 'quota').length;
  assert.ok(burstDelivered <= CAP, `parallel burst overshot the cap: ${burstDelivered} > ${CAP}`);
  assert.ok(mail.delivered.size <= CAP, `overshot the cap: ${mail.delivered.size} > ${CAP}`);
  assert.ok(limit.daily_sent_count <= CAP, `counter overshot the cap: ${limit.daily_sent_count}`);
  assert.ok(limit.daily_sent_count >= mail.delivered.size, 'every delivered mail is counted');
  assert.equal(limit.remaining_today, CAP - limit.daily_sent_count);
  assert.deepEqual(unresolved, [], 'every command ends in a final result');
  assert.equal(ok, mail.delivered.size, 'every successful command delivered exactly its mail');
  assert.equal(ok + quota, N, 'every other command is blocked by the daily limit');
});

// BEFUND (todo, kein Invariantenbruch): Schlägt nach mail.send der Abschluss-Commit
// (outbound.message.send_approved.js:120, ctx.transaction) an einem Konflikt fehl, bleibt die
// Nachricht "approved_not_sent" mit verbrauchtem Slot. Die Wiederholung reserviert einen
// ZWEITEN Slot für dieselbe Mail (Zähler > zugestellte Mails) – das Limit wird unterschritten,
// nie überschritten. Rust duldete dasselbe als "leaked-but-counted slot".
test('BEFUND INV4: a commit conflict after mail.send leaks a daily slot', { todo: 'Befund INV4-B2, siehe INVARIANTS.md' }, async () => {
  const { mail, t } = await parallelBurstWithRetries();
  assert.equal(t.doc('outbound_account_limits', SENDER).daily_sent_count, mail.delivered.size);
});
