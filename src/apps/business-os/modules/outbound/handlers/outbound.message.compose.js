// outbound.message.compose — Sammelbefehl der Mail-Verfasskette (Strang MAIL2, V4-PORT mail OFFEN 3).
//
// Ersetzt die Einzelkette, die die mail-UI je Empfänger in Reihe absetzte:
//   outbound.engagement.create → outbound.message.prepare
//     [→ outbound.message.request_approval]            (mode 'approval' | 'send')
//     [→ outbound.message.approve → send_approved]      (mode 'send')
// Keine eigene Fachlogik: jeder Schritt ist der unveränderte Einzel-Handler (gleiche Validierungen, gleiche
// Feldschreibungen, gleiche Ids). Die Teilbefehle tragen genau die Payload, die die mail-UI vorher gesendet hat
// (inkl. record_id/client_context, die die Handler in die Datensätze übernehmen), damit das Ergebnis identisch ist.
//
// Atomarität: alle Datensatz-Schreibungen aller Empfänger laufen in EINER ctx.transaction (verschachtelte
// ctx.transaction der Einzel-Handler laufen im selben tx). Capability-Aufrufe innerhalb der Kette (Audit-Ereignis
// der Freigabe) werden bis nach dem Commit zurückgestellt, damit ein Rollback keine Nebenwirkung hinterlässt.
// Der Versand (mode 'send') ist Transport mit eigener Fehlerbuchhaltung (send_approved schreibt den Fehler und
// wirft danach) und läuft deshalb je Nachricht NACH dem Commit – genau wie vorher als eigener Befehl.
//
// Payload: { campaign_id, sender_account_id, subject, body_text, body_html, mode: 'draft'|'approval'|'send',
//            content_revision_id?, created_at_ms?, client_context?,
//            recipients: [{ recipient_email, engagement_id?, message_id? }] | string[] }
import { fail, str, stableDigest } from './_lib/outbound.js';
import createEngagement from './outbound.engagement.create.js';
import prepareMessage from './outbound.message.prepare.js';
import requestApproval from './outbound.message.request_approval.js';
import approveMessage from './outbound.message.approve.js';
import sendApproved from './outbound.message.send_approved.js';

const MODES = new Set(['draft', 'approval', 'send']);

function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function normalizeRecipients(command) {
  const raw = command.payload?.recipients;
  if (!Array.isArray(raw) || raw.length === 0) fail('invalid', 'recipients is required');
  return raw.map((entry, index) => {
    const item = typeof entry === 'string' ? { recipient_email: entry } : (isObject(entry) ? entry : {});
    const recipient = str(item, ['recipient_email']);
    if (recipient == null) fail('invalid', `recipients[${index}].recipient_email is required`);
    if (typeof command.id !== 'string' && (str(item, ['engagement_id']) == null || str(item, ['message_id']) == null)) {
      fail('invalid', 'record id is required');
    }
    return {
      recipient,
      engagementId: str(item, ['engagement_id']) ?? `eng_${stableDigest(`${command.id}:${index}`)}`,
      messageId: str(item, ['message_id']) ?? `msg_${stableDigest(`${command.id}:${index}`)}`,
    };
  });
}

/** Payloads exactly as the mail UI built them (index.js buildComposeCommandBundle + dispatchCommand). */
export function composeStepPayloads(payload, recipient, now) {
  const accountKey = str(payload, ['sender_account_id']) ?? '';
  const campaignId = payload.campaign_id;
  const createdAt = typeof payload.created_at_ms === 'number' ? payload.created_at_ms : now;
  const context = isObject(payload.client_context) ? { client_context: payload.client_context } : {};
  const withRecord = (body, recordId) => ({ ...body, record_id: recordId, ...context });
  const { engagementId, messageId } = recipient;
  return {
    engagement: withRecord({
      engagement_id: engagementId,
      campaign_id: campaignId,
      sender_account_id: accountKey,
      contact_email: recipient.recipient,
      status: 'assigned',
      payload: { source: 'mail_app', recipient_email: recipient.recipient },
      created_at_ms: createdAt,
    }, engagementId),
    message: withRecord({
      message_id: messageId,
      engagement_id: engagementId,
      campaign_id: campaignId,
      channel: 'email',
      message_type: 'initial',
      direction: 'outbound',
      sender_account_id: accountKey,
      communication_account_key: accountKey,
      recipient_email: recipient.recipient,
      subject: payload.subject ?? '',
      body_text: payload.body_text ?? '',
      body_html: payload.body_html ?? '',
      payload: {
        source: 'mail_app',
        sender_address: accountKey.replace(/^email:/, ''),
        mail_content_revision_id: String(payload.content_revision_id ?? ''),
        tracking_enabled: true,
      },
      created_at_ms: createdAt,
    }, messageId),
    approval: withRecord({ message_id: messageId }, messageId),
    approve: withRecord({ message_id: messageId }, messageId),
    send: withRecord({ message_id: messageId }, messageId),
  };
}

function subCommand(command, type, payload, step, index) {
  return {
    id: `${command.id ?? 'compose'}:${step}:${index}`,
    type,
    payload,
    actor: command.actor,
    app_id: command.app_id,
    app_ver: command.app_ver,
  };
}

// Capability calls made inside the transaction are queued and replayed after commit.
function deferredCaps(queue, path = []) {
  return new Proxy(() => {}, {
    get(_target, key) {
      if (typeof key !== 'string' || key === 'then') return undefined;
      return deferredCaps(queue, [...path, key]);
    },
    apply(_target, _this, args) {
      queue.push({ path, args });
      return Promise.resolve(undefined);
    },
  });
}

async function replayCaps(ctx, queue) {
  for (const { path, args } of queue) {
    let fn = ctx.cap;
    for (const key of path) fn = fn?.[key];
    if (typeof fn !== 'function') fail('unavailable', `capability ${path.join('.')} unavailable`);
    await fn(...args);
  }
}

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const mode = str(payload, ['mode']) ?? 'draft';
  if (!MODES.has(mode)) fail('invalid', `mode must be draft, approval or send`);
  const recipients = normalizeRecipients(command);
  const now = ctx.now();
  const plan = recipients.map((recipient, index) => ({ recipient, index, steps: composeStepPayloads(payload, recipient, now) }));

  const deferred = [];
  await ctx.transaction(async (tx) => {
    const inner = {
      ...ctx,
      read: tx.read,
      get: tx.get,
      put: tx.put,
      patch: tx.patch,
      remove: tx.remove,
      transaction: (fn) => fn(tx),
      cap: deferredCaps(deferred),
    };
    for (const { steps, index } of plan) {
      await createEngagement(subCommand(command, 'outbound.engagement.create', steps.engagement, 'engagement', index), inner);
      await prepareMessage(subCommand(command, 'outbound.message.prepare', steps.message, 'message', index), inner);
      if (mode === 'approval' || mode === 'send') {
        await requestApproval(subCommand(command, 'outbound.message.request_approval', steps.approval, 'request_approval', index), inner);
      }
      if (mode === 'send') {
        await approveMessage(subCommand(command, 'outbound.message.approve', steps.approve, 'approve', index), inner);
      }
    }
  });
  await replayCaps(ctx, deferred);

  const items = plan.map(({ recipient }) => ({
    engagement_id: recipient.engagementId,
    message_id: recipient.messageId,
    recipient_email: recipient.recipient,
  }));
  if (mode !== 'send') return { ok: true, mode, collection: 'outbound_messages', items };

  // Versand je Nachricht nach dem Commit (Transport + eigene Fehlerbuchhaltung, wie die Einzelbefehle).
  const failures = [];
  for (const { steps, index, recipient } of plan) {
    try {
      await sendApproved(subCommand(command, 'outbound.message.send_approved', steps.send, 'send', index), ctx);
    } catch (error) {
      failures.push({ message_id: recipient.messageId, error });
    }
  }
  if (failures.length) {
    const first = failures[0].error;
    const error = new Error(`send failed for ${failures.length} of ${plan.length} messages: ${first?.message ?? first}`);
    error.code = first?.code ?? 'unavailable';
    throw error;
  }
  return { ok: true, mode, collection: 'outbound_messages', items };
}
