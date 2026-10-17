// Origin: CTOX
// License: Apache-2.0
// ats.subject.export – ref: store_ats_commands.rs:1238-1311 @ 4794af03d (DSGVO Art. 15 Auskunft)
// Alle Datensätze aller ATS-Collections mit subject_id = X oder candidate_id = X (ohne Dubletten nach id) plus die
// Prüfspur (business_events, event_type business_os.ats.*, summary.candidate_id = X, nach observed_at_ms).
// Prüfspur lesen = Kern-Fähigkeit → ctx.cap.audit.query (neuer Wunsch). Der Export selbst wird protokolliert.
import {
  ATS_WRITABLE_COLLECTIONS, atsActorValue, fail, firstStringField, loadByField, payloadOf, recordAtsGovernanceEvent,
} from './_ats.js';

export default async function handle(command, ctx) {
  const subjectId = firstStringField(payloadOf(command), ['subject_id', 'candidate_id']);
  if (subjectId === undefined) fail('invalid', 'ats.subject.export requires subject_id');
  const now = ctx.now();
  const collections = {};
  for (const coll of ATS_WRITABLE_COLLECTIONS) {
    const records = [...await loadByField(ctx, coll, 'subject_id', subjectId)];
    for (const rec of await loadByField(ctx, coll, 'candidate_id', subjectId)) {
      const dup = typeof rec.id === 'string' && records.some((r) => r.id === rec.id);
      if (!dup) records.push(rec);
    }
    if (records.length > 0) collections[coll] = records;
  }
  if (typeof ctx?.cap?.audit?.query !== 'function') fail('unavailable', 'capability ctx.cap.audit.query is not available');
  const trail = await ctx.cap.audit.query({ command_type_prefix: 'business_os.ats.', summary: { candidate_id: subjectId }, order: 'observed_at_ms' });
  const audit = Array.isArray(trail?.events) ? trail.events : [];
  const recordCount = Object.values(collections).reduce((sum, list) => sum + list.length, 0);
  await recordAtsGovernanceEvent(ctx, command, atsActorValue(command), 'business_os.ats.subject_exported', {
    candidate_id: subjectId, record_count: recordCount, audit_event_count: audit.length,
  }, now);
  return { ok: true, subject_id: subjectId, exported_at_ms: now, record_count: recordCount, collections, audit_trail: audit };
}
