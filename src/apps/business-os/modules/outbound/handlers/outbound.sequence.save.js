// ref: store_outbound_commands.rs:211-236 ("outbound.sequence.save")
import { idFromCommand, requiredString, objectPayload, putDefaultString, putDefaultObject, putDefaultI64 } from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const sequenceId = idFromCommand(command, ['sequence_id', 'id'], 'seq');
  const campaignId = requiredString(command.payload, ['campaign_id']);
  const sequence = objectPayload(command.payload);
  sequence.id = sequenceId;
  sequence.campaign_id = campaignId;
  putDefaultString(sequence, 'name', 'Outbound Sequence');
  putDefaultString(sequence, 'strategy_text', '');
  putDefaultString(sequence, 'sequence_policy_text', '');
  putDefaultObject(sequence, 'approval_policy');
  putDefaultObject(sequence, 'payload');
  putDefaultI64(sequence, 'created_at_ms', now);
  sequence.updated_at_ms = now;
  await ctx.put('outbound_sequences', sequence);
  return { ok: true, collection: 'outbound_sequences', sequence };
}
