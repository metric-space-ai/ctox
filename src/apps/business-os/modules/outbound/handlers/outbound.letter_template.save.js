// ref: store_outbound_commands.rs:4850 outbound_handle_letter_template_save
import { str, fail, load, objectPayload, putDefaultString, putDefaultI64, asI64 } from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  // Same PATH quirk as skillbook.save: payload.template_id.id, else record_id.
  let templateId = str(command.payload, ['template_id', 'id']);
  if (templateId == null) {
    const recordId = command.record_id ?? command.payload?.record_id;
    if (typeof recordId !== 'string') fail('invalid', 'template_id is required');
    templateId = recordId;
  }
  return ctx.transaction(async (tx) => {
    const prior = await load(tx, 'outbound_letter_templates', templateId);
    const priorVersion = (prior && asI64(prior.version_number)) ?? 0;
    // Unlike skillbook.save the template is NOT merged onto the prior record.
    const record = objectPayload(command.payload);
    record.id = templateId;
    record.template_id = templateId;
    putDefaultString(record, 'title', '');
    putDefaultString(record, 'salutation', '');
    putDefaultString(record, 'body_template', '');
    putDefaultString(record, 'closing', '');
    putDefaultI64(record, 'created_at_ms', now);
    record.updated_at_ms = now;
    record.version_number = priorVersion + 1;
    await tx.put('outbound_letter_templates', record);
    return { ok: true, template: record, version_number: priorVersion + 1 };
  });
}
