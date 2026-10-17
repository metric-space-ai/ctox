// Send path helpers. ref: store_outbound_commands.rs:6402 reserve slot, :6469 release,
// :6911 outbound_enforce_send_gate, :6943 outbound_classify_send_block,
// :6982 outbound_record_send_failure, :5961 outbound_sync_email_message_to_communication
import {
  str, firstString, fail, asI64, asBool, load, payloadInsert, requiredString, messageRevision,
  requireMessageContent, hasMatchingApproval, recipientSuppressionReason, enforceAccountLimit,
  emailAccountKeyFrom, asciiLower, stableDigest,
} from './outbound.js';

const NOT_ELIGIBLE = new Set(['blocked', 'locked', 'suspended', 'disabled']);

/** ref: store_outbound_commands.rs:5952 outbound_email_address_from_account_key */
export function emailAddressFromAccountKey(accountKey) {
  const trimmed = String(accountKey).trim();
  return asciiLower(trimmed.startsWith('email:') ? trimmed.slice('email:'.length) : trimmed);
}

async function loadLimit(tx, senderAccountId) {
  const canonical = emailAccountKeyFrom(senderAccountId) ?? senderAccountId;
  const limit = (await load(tx, 'outbound_account_limits', canonical))
    ?? (await load(tx, 'outbound_account_limits', senderAccountId));
  return { canonical, limit };
}

function writeCounters(limit, next, now) {
  limit.sent_today = next;
  limit.daily_sent_count = next;
  const limitValue = asI64(limit.daily_limit);
  if (limitValue != null && limitValue > 0) limit.remaining_today = Math.max(0, limitValue - next);
  limit.updated_at_ms = now;
}

/** ref: :6402 — must run inside ONE ctx.transaction (Rust: BEGIN IMMEDIATE). */
export async function reserveAccountSendSlot(tx, senderAccountId, now) {
  const { canonical, limit } = await loadLimit(tx, senderAccountId);
  if (!limit) return;
  if (asBool(limit.blocked) === true) fail('forbidden', 'sender account is blocked for outbound communication');
  const status = str(limit, ['status']) ?? 'active';
  if (NOT_ELIGIBLE.has(status)) fail('forbidden', `sender account status \`${status}\` is not eligible to send`);
  const current = asI64(limit.sent_today) ?? asI64(limit.daily_sent_count) ?? 0;
  const next = current + 1;
  const limitValue = asI64(limit.daily_limit);
  if (limitValue != null && limitValue > 0 && !(next <= limitValue)) fail('quota', 'sender account daily limit exhausted');
  writeCounters(limit, next, now);
  // Rust writes under the CANONICAL id even when the record was found under the raw id.
  limit.id = canonical;
  await tx.put('outbound_account_limits', limit);
}

/** ref: :6469 */
export async function releaseAccountSendSlot(tx, senderAccountId, now) {
  const { canonical, limit } = await loadLimit(tx, senderAccountId);
  if (!limit) return;
  const current = asI64(limit.sent_today) ?? asI64(limit.daily_sent_count) ?? 0;
  writeCounters(limit, Math.max(0, current - 1), now);
  limit.id = canonical;
  await tx.put('outbound_account_limits', limit);
}

/** ref: :6911 outbound_enforce_send_gate */
export async function enforceSendGate(tx, message) {
  const messageId = requiredString(message, ['id']);
  if (str(message, ['approval_status']) !== 'approved') fail('conflict', 'outbound message must be approved before send');
  const revisionId = str(message, ['revision_id']) ?? messageRevision(message);
  // The approval must cover the content that is about to go out (INV1-B1): a stored
  // revision_id alone is not trusted, see hasMatchingApproval/approvalCoversContent.
  if (!(await hasMatchingApproval(tx, messageId, revisionId, message))) {
    fail('conflict', 'approved outbound message has no matching approval for current revision');
  }
  const channel = str(message, ['channel']) ?? 'email';
  requireMessageContent(message);
  if (channel === 'physical_letter') {
    if (str(message, ['recipient_address_text']) == null) {
      fail('invalid', 'physical_letter messages require recipient_address_text before send');
    }
    return;
  }
  const senderAccountId = requiredString(message, ['sender_account_id']);
  const recipientEmail = requiredString(message, ['recipient_email']);
  const reason = await recipientSuppressionReason(tx, recipientEmail);
  if (reason != null) fail('conflict', `recipient is suppressed for outbound communication (reason: ${reason})`);
  await enforceAccountLimit(tx, senderAccountId);
}

/** ref: :6943 outbound_classify_send_block */
export function classifySendBlock(error) {
  const lowered = asciiLower(error);
  if (lowered.includes('suppress')) return 'recipient_suppressed';
  if (lowered.includes('blocked') || lowered.includes('not eligible')) return 'sender_blocked';
  if (lowered.includes('limit')) return 'sender_limit_exhausted';
  if (lowered.includes('approv')) return 'approval_required';
  if (lowered.includes('queue') || lowered.includes('provider')) return 'provider_queue_failed';
  if (lowered.includes('recipient_address') || lowered.includes('recipient_email')) return 'missing_recipient';
  if (lowered.includes('sender_account')) return 'missing_sender';
  return 'send_blocked';
}

/** ref: :6982 outbound_record_send_failure */
export async function recordSendFailure(tx, messageId, message, error, now) {
  const reason = classifySendBlock(error);
  const attempts = (asI64(message.payload?.send_attempts) ?? 0) + 1;
  message.send_status = 'send_blocked';
  payloadInsert(message, 'send_block_reason', reason);
  payloadInsert(message, 'last_send_error', error);
  payloadInsert(message, 'send_attempts', attempts);
  payloadInsert(message, 'last_send_attempt_at_ms', now);
  payloadInsert(message, 'retryable', true);
  message.updated_at_ms = now;
  await tx.put('outbound_messages', message);
  const engagementId = str(message, ['engagement_id']);
  if (engagementId != null && engagementId.trim()) {
    const engagement = await load(tx, 'outbound_engagements', engagementId);
    if (engagement) {
      engagement.status = 'send_blocked';
      engagement.last_send_block_reason = reason;
      engagement.last_send_error = error;
      engagement.last_send_block_at_ms = now;
      engagement.updated_at_ms = now;
      await tx.put('outbound_engagements', engagement);
    }
  }
}

// ---------------------------------------------------------------------------
// Kommunikationszeilen (Strang CAPSIG). communication_messages/communication_threads gehören der App
// conversations (reine Daten, keine eigenen Befehle/Handler). Statt der früheren Kern-Fähigkeiten
// mail.communication.upsert / mail.message.annotate schreibt outbound die Zeilen direkt über
// foreign_collections {owner_app:'conversations', ops:['read','write']} + exports der App conversations
// (Entscheidung: Datenpflege fremder Collections ist kein OS-Primitiv; ein conversations-Befehl hätte eine
// Handler-Schicht in conversations nur für Durchreichen erzwungen). Zeilenform = conversations-Schema
// (id = message_key bzw. thread_key, §16.28); Thread-Kennzahlen wie threads.rs refresh_thread_states.

const isoAt = (ms) => new Date(ms).toISOString();

/** ref: threads.rs:3765 refresh_thread_states – Thread-Zeile aus seinen Nachrichten neu berechnen. */
export async function refreshCommunicationThread(ctx, threadKey, now) {
  const messages = await ctx.read('communication_messages', { selector: { thread_key: threadKey } });
  if (messages.length === 0) return null;
  const latest = messages.reduce((a, b) => (String(b.external_created_at ?? '') >= String(a.external_created_at ?? '') ? b : a));
  const participants = new Set();
  for (const m of messages) {
    if (m.sender_address) participants.add(m.sender_address);
    for (const r of Array.isArray(m.recipient_addresses_json) ? m.recipient_addresses_json : []) participants.add(r);
  }
  const existing = await ctx.get('communication_threads', threadKey);
  const thread = {
    ...(existing ?? {}),
    id: threadKey,
    thread_key: threadKey,
    channel: latest.channel,
    account_key: latest.account_key,
    subject: latest.subject ?? existing?.subject ?? '',
    last_message_at: latest.external_created_at,
    last_message_key: latest.message_key,
    message_count: messages.length,
    unread_count: messages.filter((m) => !m.seen).length,
    participant_keys_json: [...participants].sort(),
    metadata_json: existing?.metadata_json ?? {},
    updated_at: isoAt(now),
  };
  await ctx.put('communication_threads', thread);
  return thread;
}

/** Nachricht in communication_messages anlegen/ersetzen (id = message_key) und den Thread auffrischen. */
export async function upsertCommunicationMessage(ctx, row) {
  const now = ctx.now();
  const existing = await ctx.get('communication_messages', row.message_key);
  const bodyText = row.body_text ?? '';
  await ctx.put('communication_messages', {
    ...(existing ?? {}),
    id: row.message_key,
    message_key: row.message_key,
    channel: row.channel,
    account_key: row.account_key,
    thread_key: row.thread_key,
    remote_id: row.remote_id,
    direction: row.direction,
    folder_hint: row.folder_hint,
    sender_display: row.sender_display ?? '',
    sender_address: row.sender_address,
    recipient_addresses_json: row.recipient_addresses ?? [],
    cc_addresses_json: row.cc_addresses ?? [],
    bcc_addresses_json: row.bcc_addresses ?? [],
    subject: row.subject ?? '',
    preview: [...bodyText].slice(0, 240).join(''),
    body_text: bodyText,
    body_html: row.body_html ?? '',
    trust_level: row.trust_level,
    status: row.status,
    seen: row.seen ? 1 : 0,
    has_attachments: row.has_attachments ? 1 : 0,
    external_created_at: existing?.external_created_at ?? isoAt(now),
    observed_at: isoAt(now),
    metadata_json: { ...(existing?.metadata_json ?? {}), ...(row.metadata ?? {}) },
  });
  await refreshCommunicationThread(ctx, row.thread_key, now);
}

/** ref: store_outbound_commands.rs annotate (Rust: Metadaten in communication_messages zusammenführen). */
export async function annotateCommunicationMessage(ctx, messageKey, metadata) {
  const existing = (await ctx.get('communication_messages', messageKey))
    ?? (await ctx.read('communication_messages', { selector: { message_key: messageKey }, limit: 1 }))[0];
  if (!existing) {
    throw Object.assign(new Error(`communication message ${messageKey} not found`), { code: 'not_found' });
  }
  await ctx.patch('communication_messages', existing.id, { metadata_json: { ...(existing.metadata_json ?? {}), ...metadata } });
}

/**
 * ref: :5961 outbound_sync_email_message_to_communication. Key derivation is
 * Fachlogik (here); the row write goes straight into communication_messages
 * (+ thread refresh) of the app conversations (upsertCommunicationMessage).
 * Mutates `message` like Rust. Returns false when the message is not an email
 * message of an `email:` account (Rust: early Ok(())).
 */
export async function syncEmailMessageToCommunication(ctx, message, status) {
  const accountKey = emailAccountKeyFrom(firstString([
    str(message, ['communication_account_key']),
    str(message, ['payload', 'communication_account_key']),
    str(message, ['sender_account_id']),
  ]));
  if (accountKey == null) return false;
  const accountAddress = emailAddressFromAccountKey(accountKey);
  if (!accountKey.startsWith('email:') || !accountAddress.includes('@')) return false;
  const recipientEmail = str(message, ['recipient_email']);
  if (recipientEmail == null) return false;
  const messageId = str(message, ['id']);
  if (messageId == null) return false;
  const engagementId = str(message, ['engagement_id']) ?? '';
  const campaignId = str(message, ['campaign_id']) ?? '';
  const subject = str(message, ['subject']) ?? '';
  const bodyText = str(message, ['body_text']) ?? '';
  const bodyHtml = str(message, ['body_html']) ?? '';
  const messageKey = firstString([
    str(message, ['communication_message_key']),
    str(message, ['payload', 'communication_message_key']),
  ]) ?? `email:${accountAddress}:outbound:${stableDigest(messageId)}`;
  const threadKey = firstString([
    str(message, ['thread_key']),
    str(message, ['payload', 'thread_key']),
  ]) ?? `email:${accountAddress}:outbound-thread:${stableDigest(`${accountKey}|${campaignId}|${engagementId}|${recipientEmail}`)}`;
  const remoteId = firstString([
    str(message, ['provider_message_id']),
    str(message, ['payload', 'provider_message_id']),
    str(message, ['payload', 'provider_queue_id']),
  ]) ?? messageId;
  const metadata = {
    source: 'business-os.outbound',
    campaign_id: campaignId,
    outbound_campaign_id: campaignId,
    engagement_id: engagementId,
    outbound_engagement_id: engagementId,
    outbound_message_id: messageId,
    communication_account_key: accountKey,
    communication_thread_key: threadKey,
    communication_message_key: messageKey,
    approval_status: str(message, ['approval_status']) ?? '',
    send_status: str(message, ['send_status']) ?? '',
    provider_dispatch_status: str(message, ['payload', 'provider_dispatch_status']) ?? 'not_dispatched',
    provider_queue_id: firstString([str(message, ['payload', 'provider_queue_id']), str(message, ['provider_message_id'])]) ?? '',
    provider_send_executed: asBool(message.payload?.provider_send_executed) ?? false,
  };
  await upsertCommunicationMessage(ctx, {
    message_key: messageKey,
    channel: 'email',
    account_key: accountKey,
    thread_key: threadKey,
    remote_id: remoteId,
    direction: 'outbound',
    folder_hint: 'outbound',
    sender_display: '',
    sender_address: accountAddress,
    recipient_addresses: [recipientEmail],
    subject,
    body_text: bodyText,
    body_html: bodyHtml,
    trust_level: 'business-os',
    status,
    seen: true,
    metadata,
  });
  message.sender_account_id = accountKey;
  message.communication_account_key = accountKey;
  message.communication_message_key = messageKey;
  message.thread_key = threadKey;
  payloadInsert(message, 'communication_account_key', accountKey);
  payloadInsert(message, 'communication_message_key', messageKey);
  payloadInsert(message, 'thread_key', threadKey);
  return true;
}
