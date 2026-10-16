// ref: store_outbound_commands.rs:57-136 ("outbound.pipeline.write_outreach_draft")
// Writeback target of the agent task `outbound.pipeline.outreach_draft`.
import { requiredFromPayloadOrRecord, loadRequired, isObject, fail, asI64 } from './_lib/outbound.js';

const MESSAGE_KEYS = ['message_mail_subject', 'message_mail_body', 'message_followup_1', 'message_followup_2'];

export default async function handle(command, ctx) {
  const now = ctx.now();
  const pipelineId = requiredFromPayloadOrRecord(command, ['pipeline_id', 'id'], 'pipeline_id is required');
  // Rust: .and_then(Value::as_i64).unwrap_or(0) — non-integers become 0.
  const contactIndex = asI64(command.payload?.contact_index) ?? 0;
  if (contactIndex < 0) fail('invalid', 'contact_index must be >= 0');
  const messages = command.payload?.messages;
  if (!isObject(messages)) fail('invalid', 'messages object is required');
  return ctx.transaction(async (tx) => {
    const item = await loadRequired(tx, 'outbound_pipeline_items', pipelineId, 'pipeline item not found');
    if (!Array.isArray(item.contacts)) fail('invalid', 'pipeline item has no contacts array');
    if (contactIndex >= item.contacts.length) fail('invalid', 'contact_index out of range');
    if (!isObject(item.contacts[contactIndex])) item.contacts[contactIndex] = {};
    const contact = item.contacts[contactIndex];
    if (!isObject(contact.messages)) contact.messages = {};
    for (const key of MESSAGE_KEYS) {
      if (typeof messages[key] === 'string') contact.messages[key] = messages[key];
    }
    contact.outreach_generating = false;
    contact.outreach_status = 'drafted';
    item.updated_at_ms = now;
    await tx.put('outbound_pipeline_items', item);
    return {
      ok: true,
      collection: 'outbound_pipeline_items',
      pipeline_id: pipelineId,
      contact_index: contactIndex,
      messages,
    };
  });
}
