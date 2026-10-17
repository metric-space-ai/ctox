// ref: store_outbound_commands.rs:4955 outbound_handle_scheduler_tick
//      + :5106 outbound_prepare_due_followup_draft, :5444 outbound_engagement_sequence_context
import {
  str, firstString, fail, asI64, asBool, loadByStringField, latestMessage, objectPayload,
  putDefaultObject, putDefaultI64, payloadInsert, messageRevision, recipientSuppressed,
  enforceAccountLimit, requiredString, newSimpleUuid,
} from './_lib/outbound.js';
import { DEFAULT_SKILLBOOK, skillbookGuidance, generateAutomatedDraft } from './_lib/drafting.js';
import { reconcileProviderOutcomes } from './_lib/reconcile.js';

const HALTED = new Set(['closed', 'cancelled', 'meeting_booked', 'paused', 'reply_received', 'bounced', 'unsubscribed', 'suppressed']);
const CAMPAIGN_HALTED = new Set(['paused', 'closed', 'setup_required']);

function sequenceContext(engagement) {
  const sequenceId = firstString([str(engagement, ['payload', 'sequence_id']), str(engagement, ['sequence_id'])]);
  const snapshot = engagement?.payload?.sequence_snapshot;
  const version = asI64(snapshot?.updated_at_ms) ?? asI64(snapshot?.version) ?? 0;
  return { sequenceId, version };
}

async function prepareDueFollowupDraft(tx, ctx, engagement, command, now) {
  const payload = command.payload ?? {};
  const engagementId = requiredString(engagement, ['id']);
  const campaignId = requiredString(engagement, ['campaign_id']);
  const draftKind = firstString([str(payload, ['draft_kind']), str(engagement, ['payload', 'next_draft_kind'])]) ?? 'followup';
  const channel = firstString([
    str(payload, ['channel']), str(engagement, ['payload', 'channel']), str(engagement, ['channel']),
  ]) ?? 'email';

  const previous = await loadByStringField(tx, 'outbound_messages', 'engagement_id', engagementId);
  const hasPendingDraft = previous.some((message) => {
    const approval = str(message, ['approval_status']) ?? '';
    const send = str(message, ['send_status']) ?? '';
    const type = str(message, ['message_type']) ?? '';
    return approval === 'awaiting_approval' && send !== 'cancelled' && send !== 'blocked'
      && (type === draftKind || type.startsWith('followup'));
  });
  if (hasPendingDraft) {
    engagement.next_action_at_ms = 0;
    payloadInsert(engagement, 'scheduler_last_skip_reason', 'draft_already_awaiting_approval');
    engagement.updated_at_ms = now;
    await tx.put('outbound_engagements', engagement);
    return { kind: 'followup_skipped_existing_draft', engagement_id: engagementId };
  }

  let senderAccountId;
  let recipientEmail;
  let recipientAddressText = '';
  if (channel === 'physical_letter') {
    recipientAddressText = firstString([
      str(payload, ['recipient_address_text']),
      str(engagement, ['payload', 'contact_address_text']),
      str(engagement, ['payload', 'recipient_address_text']),
    ]);
    if (recipientAddressText == null) fail('invalid', 'recipient_address_text is required for physical_letter scheduler drafts');
    senderAccountId = firstString([str(payload, ['sender_account_id']), str(engagement, ['sender_account_id'])]) ?? '';
    recipientEmail = firstString([str(payload, ['recipient_email']), str(engagement, ['payload', 'contact_email'])]) ?? '';
  } else {
    senderAccountId = firstString([str(payload, ['sender_account_id']), str(engagement, ['sender_account_id'])]);
    if (senderAccountId == null) fail('invalid', 'sender_account_id is required for scheduler draft');
    recipientEmail = firstString([str(payload, ['recipient_email']), str(engagement, ['payload', 'contact_email'])]);
    if (recipientEmail == null) fail('invalid', 'recipient_email is required for scheduler draft');
    if (await recipientSuppressed(tx, recipientEmail)) fail('conflict', 'recipient is suppressed for outbound communication');
  }

  if (channel !== 'physical_letter' && senderAccountId) {
    try {
      await enforceAccountLimit(tx, senderAccountId);
    } catch (limitError) {
      const detail = limitError.message;
      payloadInsert(engagement, 'scheduler_last_skip_reason', 'account_limit');
      payloadInsert(engagement, 'scheduler_last_skip_detail', detail);
      payloadInsert(engagement, 'scheduler_last_skip_at_ms', now);
      engagement.updated_at_ms = now;
      await tx.put('outbound_engagements', engagement);
      return { kind: 'followup_skipped_account_limit', engagement_id: engagementId, sender_account_id: senderAccountId, reason: detail };
    }
  }

  const latest = latestMessage(previous);
  const skillbookId = firstString([str(payload, ['skillbook_id']), str(engagement, ['payload', 'skillbook_id'])]) ?? DEFAULT_SKILLBOOK;
  const guidance = await skillbookGuidance(tx, skillbookId);
  const generated = generateAutomatedDraft(engagement, latest, payload, draftKind, guidance);
  const subject = firstString([str(payload, ['subject']), str(generated, ['subject'])]);
  if (subject == null) fail('invalid', 'generated subject is required');
  const bodyText = firstString([str(payload, ['body_text']), str(generated, ['body_text'])]);
  if (bodyText == null) fail('invalid', 'generated body_text is required');
  const messageId = `msg_${newSimpleUuid(ctx, command, engagementId)}`;
  // Rust copies the WHOLE tick payload into every generated message.
  const message = objectPayload(payload);
  message.id = messageId;
  message.engagement_id = engagementId;
  message.campaign_id = campaignId;
  message.message_type = draftKind;
  message.channel = channel;
  message.direction = 'outbound';
  message.sender_account_id = senderAccountId;
  message.recipient_email = recipientEmail;
  if (recipientAddressText) message.recipient_address_text = recipientAddressText;
  message.subject = subject;
  message.body_text = bodyText;
  message.draft_status = 'ready_for_review';
  message.approval_status = 'awaiting_approval';
  message.send_status = 'awaiting_approval';
  putDefaultObject(message, 'payload');
  payloadInsert(message, 'draft_engine', 'business-os.outbound.scheduler.v1');
  payloadInsert(message, 'generated_draft', generated);
  const { sequenceId, version } = sequenceContext(engagement);
  if (sequenceId != null) payloadInsert(message, 'sequence_id', sequenceId);
  payloadInsert(message, 'sequence_version', version);
  const previousId = latest ? str(latest, ['id']) : null;
  if (previousId != null) message.reply_to_message_id = previousId;
  message.revision_id = messageRevision(message);
  putDefaultI64(message, 'created_at_ms', now);
  message.updated_at_ms = now;
  await tx.put('outbound_messages', message);

  engagement.status = 'awaiting_approval';
  engagement.next_action_at_ms = 0;
  payloadInsert(engagement, 'scheduler_last_message_id', messageId);
  payloadInsert(engagement, 'scheduler_last_run_at_ms', now);
  engagement.updated_at_ms = now;
  await tx.put('outbound_engagements', engagement);
  return { kind: 'followup_draft_prepared', engagement_id: engagementId, message_id: messageId, draft_kind: draftKind };
}

export default async function handle(command, ctx) {
  const now = ctx.now();
  const dryRun = asBool(command.payload?.dry_run) ?? false;
  const actions = [];

  const reconcile = dryRun
    ? { ok: true, checked: 0, updated: [], dry_run: true }
    : await reconcileProviderOutcomes(ctx, now);
  if (Array.isArray(reconcile.updated) && reconcile.updated.length) {
    actions.push({ kind: 'provider_reconcile', count: reconcile.updated.length });
  }

  const engagements = await ctx.read('outbound_engagements', { selector: {} });
  const campaignStatus = new Map();
  for (const campaign of await ctx.read('outbound_campaigns', { selector: {} })) {
    const id = str(campaign, ['id']);
    if (id != null) campaignStatus.set(id, str(campaign, ['status']) ?? 'active');
  }

  for (const engagement of engagements) {
    const status = str(engagement, ['status']) ?? '';
    const isOooWait = status === 'reply_received' && str(engagement, ['payload', 'reply_classification']) === 'out_of_office';
    if (!isOooWait && HALTED.has(status)) continue;
    const nextAction = asI64(engagement.next_action_at_ms) ?? asI64(engagement.payload?.next_action_at_ms);
    if (nextAction == null || !(nextAction <= now)) continue;
    const engagementId = str(engagement, ['id']);
    if (engagementId == null) continue;
    const campaignId = str(engagement, ['campaign_id']);
    if (campaignId != null && campaignStatus.has(campaignId) && CAMPAIGN_HALTED.has(campaignStatus.get(campaignId))) {
      actions.push({
        kind: 'followup_skipped_campaign_paused',
        engagement_id: engagementId,
        campaign_id: campaignId,
        campaign_status: campaignStatus.get(campaignId),
      });
      continue;
    }
    if (dryRun) {
      actions.push({ kind: 'followup_due', engagement_id: engagementId, due_at_ms: nextAction });
      continue;
    }
    try {
      actions.push(await ctx.transaction((tx) => prepareDueFollowupDraft(tx, ctx, engagement, command, now)));
    } catch (error) {
      actions.push({ kind: 'followup_prepare_failed', engagement_id: engagementId, error: error.message });
    }
  }

  return { ok: true, now_ms: now, actions, dry_run: dryRun, reconciled: reconcile.updated ?? [] };
}
