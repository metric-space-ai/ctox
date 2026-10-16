// customers.dedupe.resolve – ref: store_customer_commands.rs:1135-1195 (customers_dedupe_resolve) @ 4794af03d
import {
  requiredFromPayloadOrRecord, loadRequired, requiredStr, validateAllowed, actorId, putRecord, writeActivity,
  str, clone,
} from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const candidateId = requiredFromPayloadOrRecord(
      command, ['candidate_id', 'dedupe_candidate_id', 'id'], 'candidate_id is required',
    );
    const candidate = await loadRequired(tx, 'customer_dedupe_candidates', candidateId, 'dedupe candidate not found');
    const decision = requiredStr(command.payload, ['decision']);
    validateAllowed('dedupe decision', decision, ['merge', 'keep_existing', 'create_new', 'skip']);
    if (decision === 'merge') requiredStr(candidate, ['existing_record_id']);
    // TREUE: Rust hält nur die Entscheidung fest; merge/create_new verändern keine Konten (Befund B2).
    candidate.status = 'resolved';
    candidate.decision = decision;
    candidate.decided_by_id = actorId(command);
    candidate.decided_at_ms = now;
    await putRecord(tx, 'customer_dedupe_candidates', candidateId, now, candidate);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'dedupe_resolved',
      name: 'Duplikat entschieden',
      account_id: str(candidate, ['existing_record_id']),
      linked_record_type: 'dedupe_candidate',
      linked_record_id: candidateId,
      linked_record_name: str(candidate, ['match_key']),
      properties: { candidate: clone(candidate), decision },
    });
    return { ok: true, dedupe_candidate: candidate, activity };
  });
}
