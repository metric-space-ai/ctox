// ref: store_outbound_commands.rs:4602 outbound_handle_skillbook_save
import { str, fail, load, objectPayload, putDefaultString, putDefaultI64, asI64, isObject } from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  // Rust: outbound_required_string(payload, &["skillbook_id", "id"]) is a PATH
  // (payload.skillbook_id.id), so in practice the id comes from record_id.
  // Ported literally; see V4-HANDLERS.md "Offene Punkte" (P3).
  let skillbookId = str(command.payload, ['skillbook_id', 'id']);
  if (skillbookId == null) {
    const recordId = command.record_id ?? command.payload?.record_id;
    if (typeof recordId !== 'string') fail('invalid', 'skillbook_id is required');
    skillbookId = recordId;
  }
  return ctx.transaction(async (tx) => {
    const prior = await load(tx, 'outbound_skillbooks', skillbookId);
    const priorVersion = (prior && asI64(prior.version_number)) ?? 0;
    const record = prior ? prior : {};
    const incoming = objectPayload(command.payload);
    if (isObject(record)) Object.assign(record, incoming);
    record.id = skillbookId;
    record.skillbook_id = skillbookId;
    putDefaultString(record, 'title', '');
    putDefaultString(record, 'mission', '');
    for (const key of ['non_negotiable_rules', 'workflow_backbone', 'routing_taxonomy', 'stop_rules']) {
      if (!Array.isArray(record[key])) record[key] = [];
    }
    putDefaultI64(record, 'created_at_ms', now);
    record.updated_at_ms = now;
    record.version_number = priorVersion + 1;
    await tx.put('outbound_skillbooks', record);
    return { ok: true, skillbook: record, version_number: priorVersion + 1 };
  });
}
