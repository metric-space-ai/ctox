// Origin: CTOX
// License: Apache-2.0
// ats.retention.due – ref: store_ats_commands.rs:112-146 @ 4794af03d (nur lesend; Löschung ist ats.retention.purge)
// Rust las jede beliebige business_records-Collection; in v4 begrenzt das Manifest (eigene + foreign_collections
// mit `read`) – nicht deklarierte Collections → forbidden.
import { asI64, fail, payloadOf, retentionDue } from './_ats.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  if (typeof p.collection !== 'string') fail('invalid', 'ats.retention.due requires collection');
  const retentionDays = asI64(p.retention_days) ?? 0;
  const referenceField = typeof p.reference_field === 'string' ? p.reference_field : 'created_at_ms';
  const now = ctx.now();
  const rows = await ctx.read(p.collection, { selector: {} });
  const dueIds = rows.filter((row) => retentionDue(asI64(row[referenceField]) ?? 0, retentionDays, now)).map((row) => row.id);
  return { ok: true, count: dueIds.length, due_ids: dueIds };
}
