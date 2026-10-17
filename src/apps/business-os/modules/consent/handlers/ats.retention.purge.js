// Origin: CTOX
// License: Apache-2.0
// ats.retention.purge – ref: store_ats_commands.rs:343-385 @ 4794af03d
// Nur ATS-PII-Collections (ATS_WRITABLE_COLLECTIONS); fällige Datensätze werden gelöscht (DSGVO).
// Rust überschrieb den Payload vor dem Soft-Delete mit einem PII-freien Grabstein {redacted, redacted_at_ms,
// reference_field, retention_days}. v4: ctx.remove – Laufzeitanforderung: der Lösch-Grabstein des Sync-Stores
// trägt keinen Payload (siehe V4-HANDLERS.md, offene Punkte). Prüfspur-Ereignis nach der Transaktion.
import {
  asI64, atsActorValue, ensureAtsCollectionAllowed, fail, payloadOf, recordAtsGovernanceEvent, retentionDue,
} from './_ats.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  if (typeof p.collection !== 'string') fail('invalid', 'ats.retention.purge requires collection');
  ensureAtsCollectionAllowed(p.collection);
  const retentionDays = asI64(p.retention_days) ?? 0;
  const referenceField = typeof p.reference_field === 'string' ? p.reference_field : 'created_at_ms';
  const now = ctx.now();
  const purged = await ctx.transaction(async (tx) => {
    const ids = [];
    for (const row of await tx.read(p.collection, { selector: {} })) {
      if (retentionDue(asI64(row[referenceField]) ?? 0, retentionDays, now)) {
        await tx.remove(p.collection, row.id);
        ids.push(row.id);
      }
    }
    return ids;
  });
  await recordAtsGovernanceEvent(ctx, command, atsActorValue(command), 'business_os.ats.retention_purged', {
    collection: p.collection, purged_count: purged.length, purged_ids: purged, retention_days: retentionDays,
  }, now);
  return { ok: true, purged: purged.length, purged_ids: purged };
}
