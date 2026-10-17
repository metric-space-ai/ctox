// ref: store_outbound_commands.rs:5508 outbound_handle_provider_reconcile
// Fachlogik (status promotion) here; the SMTP delivery log
// (core DB table stalwart_smtp_delivery_log) is read via the core primitive
// ctx.cap.mail.delivery_log.query({message_ids}) → {deliveries:[{message_id, outcome, error_text, completed_at}]}
// (caps/mail.rs DeliveryQuery). Den jüngsten Eintrag je Nachricht wählt der Handler (größtes completed_at).
import { str, firstString, payloadInsert, updateEngagementStatus } from './outbound.js';

const RECONCILABLE = new Set(['queued_for_provider', 'queued', 'approved_not_sent']);

/** Jüngste Zustellung je message_id (Rust: ORDER BY completed_at DESC LIMIT 1 je id). */
export function latestDeliveries(deliveries) {
  const entries = {};
  for (const d of Array.isArray(deliveries) ? deliveries : []) {
    const id = typeof d?.message_id === 'string' ? d.message_id : (typeof d?.queue_id === 'string' ? d.queue_id : null);
    if (!id) continue;
    const prev = entries[id];
    if (!prev || (Number(d.completed_at) || 0) >= (Number(prev.completed_at) || 0)) {
      entries[id] = { outcome: d.outcome, error_text: d.error_text, completed_at: d.completed_at };
    }
  }
  return entries;
}

export async function reconcileProviderOutcomes(ctx, now) {
  const messages = await ctx.read('outbound_messages', { selector: {} });
  const candidates = [];
  for (const message of messages) {
    const sendStatus = str(message, ['send_status']) ?? 'draft';
    if (!RECONCILABLE.has(sendStatus)) continue;
    const providerId = firstString([
      str(message, ['provider_message_id']),
      str(message, ['payload', 'provider_queue_id']),
      str(message, ['payload', 'provider_message_id']),
    ]);
    if (providerId == null) continue;
    if (str(message, ['id']) == null) continue;
    candidates.push({ message, providerId });
  }
  const checked = candidates.length;
  let log = { available: true, entries: {} };
  if (candidates.length) {
    try {
      const ids = candidates.map((c) => c.providerId);
      const result = await ctx.cap.mail.delivery_log.query({ message_ids: ids });
      log = { available: true, entries: latestDeliveries(result?.deliveries) };
    } catch (error) {
      // Rust: missing core DB => log_available=false, not an error.
      if (error?.code !== 'unavailable') throw error;
      log = { available: false };
    }
  }
  const available = Boolean(log?.available);
  const updated = [];
  if (!available) return { ok: true, checked, updated, log_available: false };
  for (const { message, providerId } of candidates) {
    const entry = log.entries?.[providerId];
    if (!entry) continue;
    const { outcome, error_text: errorText, completed_at: completedAt } = entry;
    const newStatus = outcome === 'delivered' ? 'sent' : outcome === 'failed' ? 'failed' : outcome;
    const messageId = str(message, ['id']);
    await ctx.transaction(async (tx) => {
      message.send_status = newStatus;
      payloadInsert(message, 'provider_dispatch_status', outcome);
      payloadInsert(message, 'provider_completed_at_ms', completedAt);
      if (typeof errorText === 'string') payloadInsert(message, 'provider_error_text', errorText);
      if (newStatus === 'sent') {
        message.sent_at_ms = completedAt;
        payloadInsert(message, 'delivered_at_ms', completedAt);
      }
      message.updated_at_ms = now;
      await tx.put('outbound_messages', message);
      const engagementId = str(message, ['engagement_id']);
      if (newStatus === 'sent' && engagementId != null) await updateEngagementStatus(tx, engagementId, 'sent', now);
    });
    updated.push({ message_id: messageId, outcome, send_status: newStatus });
  }
  return { ok: true, checked, updated, log_available: true };
}
