// ref: store_outbound_commands.rs:462-500 ("outbound.message.prepare")
import {
  idFromCommand, requiredString, load, str, firstString, fail, objectPayload, putDefaultString,
  putDefaultObject, putDefaultI64, messageRevision, updateEngagementStatus,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const messageId = idFromCommand(command, ['message_id', 'id'], 'msg');
  const engagementId = requiredString(command.payload, ['engagement_id']);
  return ctx.transaction(async (tx) => {
    const engagement = await load(tx, 'outbound_engagements', engagementId);
    const campaignId = firstString([
      str(command.payload, ['campaign_id']),
      engagement ? str(engagement, ['campaign_id']) : null,
    ]);
    if (campaignId == null) fail('invalid', 'campaign_id is required');
    const message = objectPayload(command.payload);
    message.id = messageId;
    message.engagement_id = engagementId;
    message.campaign_id = campaignId;
    putDefaultString(message, 'message_type', 'initial');
    putDefaultString(message, 'direction', 'outbound');
    putDefaultString(message, 'draft_status', 'prepared');
    putDefaultString(message, 'approval_status', 'draft');
    putDefaultString(message, 'send_status', 'not_scheduled');
    putDefaultObject(message, 'payload');
    message.revision_id = messageRevision(message);
    putDefaultI64(message, 'created_at_ms', now);
    message.updated_at_ms = now;
    await tx.put('outbound_messages', message);
    await updateEngagementStatus(tx, engagementId, 'draft_prepared', now);
    return { ok: true, collection: 'outbound_messages', message };
  });
}
