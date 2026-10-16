// ref: store_outbound_commands.rs:137-159 ("outbound.engagement.create")
import { idFromCommand, requiredString, objectPayload, putDefaultString, putDefaultObject, putDefaultI64 } from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const engagementId = idFromCommand(command, ['engagement_id', 'id'], 'eng');
  const campaignId = requiredString(command.payload, ['campaign_id']);
  const record = objectPayload(command.payload);
  record.id = engagementId;
  record.campaign_id = campaignId;
  putDefaultString(record, 'status', 'ready_for_assignment');
  putDefaultObject(record, 'payload');
  putDefaultI64(record, 'created_at_ms', now);
  record.updated_at_ms = now;
  await ctx.put('outbound_engagements', record);
  return { ok: true, collection: 'outbound_engagements', engagement: record };
}
