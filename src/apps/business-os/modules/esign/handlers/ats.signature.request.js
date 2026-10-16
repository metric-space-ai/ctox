// Origin: CTOX
// License: Apache-2.0
// ats.signature.request – ref: store_ats_commands.rs:871-900 @ 4794af03d
import { atsActorValue, fail, firstStringField, payloadOf, recordAtsGovernanceEvent, upsertRecord } from './_ats.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const documentId = firstStringField(p, ['document_id']);
  if (documentId === undefined) fail('invalid', 'ats.signature.request requires document_id');
  const now = ctx.now();
  const id = `sig_${now}`;
  await upsertRecord(ctx, 'signature_requests', id, now, {
    id,
    document_id: documentId,
    subject_kind: firstStringField(p, ['subject_kind']) ?? '',
    signers: p.signers !== undefined ? p.signers : [],
    sent_at_ms: now,
    status: 'sent',
    created_at_ms: now,
    updated_at_ms: now,
    _deleted: false,
  });
  await recordAtsGovernanceEvent(ctx, command, atsActorValue(command), 'business_os.ats.signature_requested',
    { request_id: id, document_id: documentId }, now);
  return { ok: true, request_id: id, status: 'sent' };
}
