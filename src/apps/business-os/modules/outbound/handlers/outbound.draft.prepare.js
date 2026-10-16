// ref: store_outbound_commands.rs:237-461 ("outbound.draft.prepare")
// Deterministic draft automation (no model call): template text from
// outbound_generate_automated_draft :6579 plus skillbook guidance :6555.
import {
  idFromCommand, requiredString, loadRequired, str, firstString, fail, loadByStringField,
  latestMessage, objectPayload, putDefaultObject, putDefaultI64, payloadInsert, messageRevision,
  recipientSuppressed, updateEngagementStatus, asI64,
} from './_lib/outbound.js';
import { DEFAULT_SKILLBOOK, skillbookGuidance, generateAutomatedDraft } from './_lib/drafting.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const payload = command.payload ?? {};
  const messageId = idFromCommand(command, ['message_id', 'id'], 'msg');
  const engagementId = requiredString(payload, ['engagement_id']);
  return ctx.transaction(async (tx) => {
    const engagement = await loadRequired(tx, 'outbound_engagements', engagementId, 'engagement not found');
    const engagementStatus = str(engagement, ['status']);
    if (engagementStatus === 'closed' || engagementStatus === 'cancelled' || engagementStatus === 'meeting_booked') {
      fail('conflict', 'closed outbound engagements cannot prepare new drafts');
    }
    const draftKind = str(payload, ['draft_kind']) ?? 'followup';
    const campaignId = firstString([str(payload, ['campaign_id']), str(engagement, ['campaign_id'])]);
    if (campaignId == null) fail('invalid', 'campaign_id is required');
    const channel = firstString([
      str(payload, ['channel']),
      str(engagement, ['payload', 'channel']),
      str(engagement, ['payload', 'active_outreach', 'default_channel']),
      str(engagement, ['channel']),
    ]) ?? 'email';

    let senderAccountId;
    let recipientEmail;
    let recipientAddressText;
    if (channel === 'physical_letter') {
      recipientAddressText = firstString([
        str(payload, ['recipient_address_text']),
        str(payload, ['recipient_address']),
        str(engagement, ['payload', 'contact_address_text']),
        str(engagement, ['payload', 'recipient_address_text']),
      ]);
      if (recipientAddressText == null) fail('invalid', 'recipient_address_text is required for physical_letter drafts');
      senderAccountId = firstString([str(payload, ['sender_account_id']), str(engagement, ['sender_account_id'])]) ?? '';
      recipientEmail = firstString([str(payload, ['recipient_email']), str(engagement, ['payload', 'contact_email'])]) ?? '';
    } else {
      senderAccountId = firstString([str(payload, ['sender_account_id']), str(engagement, ['sender_account_id'])]);
      if (senderAccountId == null) fail('invalid', 'sender_account_id is required');
      recipientEmail = firstString([str(payload, ['recipient_email']), str(engagement, ['payload', 'contact_email'])]);
      if (recipientEmail == null) fail('invalid', 'recipient_email is required');
      if (await recipientSuppressed(tx, recipientEmail)) {
        fail('conflict', 'recipient is suppressed for outbound communication');
      }
      recipientAddressText = '';
    }

    const previous = await loadByStringField(tx, 'outbound_messages', 'engagement_id', engagementId);
    const latest = latestMessage(previous);
    const skillbookId = firstString([
      str(payload, ['skillbook_id']),
      str(payload, ['payload', 'skillbook_id']),
      str(engagement, ['payload', 'skillbook_id']),
    ]) ?? DEFAULT_SKILLBOOK;
    const guidance = await skillbookGuidance(tx, skillbookId);
    const generated = generateAutomatedDraft(engagement, latest, payload, draftKind, guidance);
    const subject = firstString([str(payload, ['subject']), str(generated, ['subject'])]);
    if (subject == null) fail('invalid', 'generated subject is required');
    const bodyText = firstString([str(payload, ['body_text']), str(generated, ['body_text'])]);
    if (bodyText == null) fail('invalid', 'generated body_text is required');

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
    payloadInsert(message, 'draft_engine', 'business-os.outbound.draft_automation.v1');
    payloadInsert(message, 'generated_draft', generated);
    payloadInsert(message, 'skillbook_id', skillbookId);
    payloadInsert(message, 'runbook_id', firstString([
      str(payload, ['runbook_id']),
      str(payload, ['payload', 'runbook_id']),
      str(engagement, ['payload', 'runbook_id']),
    ]) ?? '');
    const previousId = latest ? str(latest, ['id']) : null;
    if (previousId != null) message.reply_to_message_id = previousId;
    const proposedSlots = payload.proposed_slots !== undefined ? payload.proposed_slots : [];
    const meetingRequestId = str(payload, ['meeting_request_id']) ?? `meeting_${messageId}`;
    if (draftKind === 'scheduling') {
      payloadInsert(message, 'meeting_request_id', meetingRequestId);
      payloadInsert(message, 'proposed_slots', proposedSlots);
    }
    message.revision_id = messageRevision(message);
    putDefaultI64(message, 'created_at_ms', now);
    message.updated_at_ms = now;
    await tx.put('outbound_messages', message);

    let meetingRequest = null;
    if (draftKind === 'scheduling') {
      meetingRequest = {
        id: meetingRequestId,
        engagement_id: engagementId,
        message_id: messageId,
        duration_minutes: asI64(payload.duration_minutes) ?? 30,
        slot_strategy: str(payload, ['slot_strategy']) ?? 'campaign_default',
        proposed_slots: proposedSlots,
        status: 'prepared',
        payload: { source: 'outbound.draft.prepare' },
        created_at_ms: now,
        updated_at_ms: now,
      };
      const calendar = str(payload, ['calendar_account_id']);
      if (calendar != null) meetingRequest.calendar_account_id = calendar;
      await tx.put('outbound_meeting_requests', meetingRequest);
    }
    await updateEngagementStatus(tx, engagementId, 'awaiting_approval', now);
    return {
      ok: true,
      collection: 'outbound_messages',
      message,
      meeting_request: meetingRequest,
      approval_required: true,
      provider_send_executed: false,
    };
  });
}
