// ref: store_outbound_commands.rs:657-835 ("outbound.message.send_approved") — GEMISCHT.
// Fachlogik (gate, idempotency, daily slot, failure bookkeeping, statuses) here.
// Native: ctx.cap.mail.send (RFC822 build, mailserver queue). Tracking (Pixel, Link-Umschreibung, Tokens) ist
// App-Logik (Kern §16.35 Nr. 3, Strang TRACK): _lib/tracking.js prepareTrackedHtml, Pfad über net.ingest.
import { requiredFromPayloadOrRecord, loadRequired, str, firstString, asBool, payloadInsert, updateEngagementStatus, requiredString, fail } from './_lib/outbound.js';
import {
  enforceSendGate, recordSendFailure, reserveAccountSendSlot, releaseAccountSendSlot,
  syncEmailMessageToCommunication,
} from './_lib/send.js';
import { prepareTrackedHtml, trackingBaseForSend } from './_lib/tracking.js';

const QUEUED = new Set(['queued_for_provider', 'sent', 'accepted']);

async function failWith(ctx, messageId, message, error, now, recordedMessage = error.message) {
  // Rust persists the failure and then returns the error: commit first, throw after.
  await ctx.transaction((tx) => recordSendFailure(tx, messageId, message, recordedMessage, now));
  throw error;
}

export default async function handle(command, ctx) {
  const now = ctx.now();
  const messageId = requiredFromPayloadOrRecord(command, ['message_id', 'id'], 'message_id is required');
  const message = await loadRequired(ctx, 'outbound_messages', messageId, 'message not found');

  try {
    await enforceSendGate(ctx, message);
  } catch (error) {
    if (!error?.code) throw error;
    await failWith(ctx, messageId, message, error, now);
  }
  const channel = str(message, ['channel']) ?? 'email';

  if (channel === 'physical_letter') {
    if ((str(message, ['payload', 'provider_dispatch_status']) ?? '') === 'manual_physical_letter_marked_sent') {
      return { ok: true, message, channel: 'physical_letter', provider_dispatch_status: 'manual_physical_letter_marked_sent', idempotent: true };
    }
    return ctx.transaction(async (tx) => {
      message.send_status = 'sent';
      payloadInsert(message, 'provider_dispatch_status', 'manual_physical_letter_marked_sent');
      payloadInsert(message, 'provider_send_executed', true);
      payloadInsert(message, 'physical_sent_at_ms', now);
      message.sent_at_ms = now;
      message.updated_at_ms = now;
      await tx.put('outbound_messages', message);
      const engagementId = str(message, ['engagement_id']);
      if (engagementId != null) await updateEngagementStatus(tx, engagementId, 'sent', now);
      return {
        ok: true, message, channel: 'physical_letter', provider_dispatch_status: 'manual_physical_letter_marked_sent',
        provider_send_executed: true, physical_sent_at_ms: now,
      };
    });
  }

  const existingQueueId = firstString([
    str(message, ['provider_message_id']),
    str(message, ['payload', 'provider_queue_id']),
    str(message, ['payload', 'provider_message_id']),
  ]);
  const existingSendStatus = str(message, ['send_status']) ?? '';
  if (QUEUED.has(existingSendStatus) && existingQueueId != null && asBool(message.payload?.provider_send_executed) === true) {
    // Rust re-syncs the communication row but does NOT persist the message here.
    await syncEmailMessageToCommunication(ctx, message, existingSendStatus);
    return {
      ok: true, message, provider_dispatch_status: 'queued_in_mailserver', provider_queue_id: existingQueueId,
      provider_send_executed: true, idempotent: true,
    };
  }

  const senderAccountId = requiredString(message, ['sender_account_id']);
  try {
    await ctx.transaction((tx) => reserveAccountSendSlot(tx, senderAccountId, now));
  } catch (error) {
    if (!error?.code) throw error;
    await failWith(ctx, messageId, message, error, now);
  }

  let providerQueueId;
  try {
    const recipient = requiredString(message, ['recipient_email']);
    const bodyText = str(message, ['body_text']) ?? '';
    let bodyHtml = str(message, ['body_html']) ?? '';
    if (!bodyText.trim() && !bodyHtml.trim()) fail('invalid', 'outbound email body is empty (body_text and body_html both blank)');
    // ref: store_outbound_commands.rs:6116-6118 – Tracking nur bei HTML-Körper; die Nachricht selbst behält das
    // ungetrackte HTML (auch in communication_messages), nur der Versand trägt Pixel und umgeschriebene Links.
    if (bodyHtml.trim()) bodyHtml = await prepareTrackedHtml(ctx, command, message, bodyHtml, await trackingBaseForSend(ctx));
    // Kernform caps/mail.rs SendRequest: account_id = konfiguriertes Absenderkonto (der Kern löst die Adresse
    // selbst auf, kein `from`); Ergebnis {message_id, status}. Tracking ist nicht Teil des Primitivs (§16.35 Nr. 3).
    const sent = await ctx.cap.mail.send({
      account_id: senderAccountId,
      to: [recipient],
      subject: str(message, ['subject']) ?? '',
      text: bodyText,
      html: bodyHtml,
      idempotency_key: `${messageId}:${str(message, ['revision_id']) ?? ''}`,
    });
    providerQueueId = sent?.message_id ?? sent?.queue_id;
    if (typeof providerQueueId !== 'string' || !providerQueueId) fail('unavailable', 'provider queue returned no queue id');
  } catch (error) {
    const wrapped = Object.assign(new Error(`failed to queue approved outbound email: ${error?.message ?? error}`), { code: error?.code ?? 'unavailable' });
    await ctx.transaction((tx) => releaseAccountSendSlot(tx, senderAccountId, now)).catch(() => {});
    // Rust records err.to_string() of the anyhow context = outer text only. Its
    // classifier then hits "approv" in "approved" => approval_required (quirk P8).
    await failWith(ctx, messageId, message, wrapped, now, 'failed to queue approved outbound email');
  }

  message.provider_message_id = providerQueueId;
  payloadInsert(message, 'provider_queue_id', providerQueueId);
  payloadInsert(message, 'provider_message_id', providerQueueId);
  payloadInsert(message, 'provider_dispatch_status', 'queued_in_mailserver');
  payloadInsert(message, 'provider_send_executed', true);
  payloadInsert(message, 'provider_queued_at_ms', now);
  message.send_status = 'queued_for_provider';
  payloadInsert(message, 'send_block_reason', null);
  payloadInsert(message, 'last_send_error', null);
  payloadInsert(message, 'retryable', false);
  await syncEmailMessageToCommunication(ctx, message, 'queued_for_provider');
  message.updated_at_ms = now;
  return ctx.transaction(async (tx) => {
    await tx.put('outbound_messages', message);
    const engagementId = str(message, ['engagement_id']);
    if (engagementId != null) await updateEngagementStatus(tx, engagementId, 'scheduled_to_send', now);
    return {
      ok: true, message, provider_dispatch_status: 'queued_in_mailserver',
      provider_queue_id: str(message, ['payload', 'provider_queue_id']), provider_send_executed: true,
    };
  });
}
