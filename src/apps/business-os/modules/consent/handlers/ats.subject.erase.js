// Origin: CTOX
// License: Apache-2.0
// ats.subject.erase – ref: store_ats_commands.rs:1312-1372 @ 4794af03d (DSGVO Art. 17 Löschung)
// Je ATS-Collection alle ids mit subject_id = X oder candidate_id = X; alle in EINER Transaktion löschen
// (Rust: Grabstein {redacted, erasure_basis:'dsgvo_art17'} + deleted=1, je Zeile). Ereignis danach.
import {
  ATS_WRITABLE_COLLECTIONS, atsActorValue, fail, firstStringField, loadByField, payloadOf, recordAtsGovernanceEvent,
} from './_ats.js';

export default async function handle(command, ctx) {
  const subjectId = firstStringField(payloadOf(command), ['subject_id', 'candidate_id']);
  if (subjectId === undefined) fail('invalid', 'ats.subject.erase requires subject_id');
  const now = ctx.now();
  const erased = await ctx.transaction(async (tx) => {
    const out = {};
    for (const coll of ATS_WRITABLE_COLLECTIONS) {
      const ids = [];
      for (const field of ['subject_id', 'candidate_id']) {
        for (const rec of await loadByField(tx, coll, field, subjectId)) {
          if (typeof rec.id === 'string' && !ids.includes(rec.id)) ids.push(rec.id);
        }
      }
      for (const id of ids) await tx.remove(coll, id);
      if (ids.length > 0) out[coll] = ids;
    }
    return out;
  });
  const erasedCount = Object.values(erased).reduce((sum, list) => sum + list.length, 0);
  await recordAtsGovernanceEvent(ctx, command, atsActorValue(command), 'business_os.ats.subject_erased', {
    candidate_id: subjectId, erased_count: erasedCount,
  }, now);
  return { ok: true, subject_id: subjectId, erased_at_ms: now, erased_count: erasedCount, erased };
}
