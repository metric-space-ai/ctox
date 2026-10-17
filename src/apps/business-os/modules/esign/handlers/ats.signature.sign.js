// Origin: CTOX
// License: Apache-2.0
// ats.signature.sign – ref: store_ats_commands.rs:901-1003 @ 4794af03d (H11)
// Nur ein authentifizierter Handelnder (command.actor.id, nicht 'rxdb-command') signiert; Unterzeichner wird mit
// signed_by_actor_id/signed_at_ms gestempelt, Gesamtstatus über signature_request_status, bei `completed`
// unveränderliche Artefakt-ID sig_<sha256(request|actor|now|signers-JSON)>.
import {
  asI64, atsActorValue, authenticatedActorId, fail, firstStringField, payloadOf, recordAtsGovernanceEvent,
  signatureRequestStatus, upsertRecord,
} from './_ats.js';
import { sha256Hex } from './_sha256.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const requestId = firstStringField(p, ['request_id']);
  if (requestId === undefined) fail('invalid', 'ats.signature.sign requires request_id');
  const signerId = firstStringField(p, ['signer_id']);
  if (signerId === undefined) fail('invalid', 'ats.signature.sign requires signer_id');
  const actorId = authenticatedActorId(command, 'ats.signature.sign requires an authenticated actor');
  const now = ctx.now();
  const out = await ctx.transaction(async (tx) => {
    const existing = await tx.get('signature_requests', requestId);
    if (!existing) fail('not_found', 'signature request not found');
    const payload = JSON.parse(JSON.stringify(existing));
    let signedAny = false;
    if (Array.isArray(payload.signers)) {
      for (const signer of payload.signers) {
        if (signer && typeof signer === 'object' && !Array.isArray(signer) && signer.id === signerId) {
          signer.state = 'signed';
          signer.signed_by_actor_id = actorId;
          signer.signed_at_ms = now;
          signedAny = true;
        }
      }
    }
    if (!signedAny) fail('invalid', 'signer_id does not match any signer on this request');
    const signers = payload.signers;
    const status = signatureRequestStatus(signers, asI64(payload.expires_at_ms), asI64(payload.sent_at_ms), now);
    // serde_json::to_string: kompakt, Schlüssel in Einfügereihenfolge (preserve_order über src/core/rxdb/Cargo.toml:25,
    // Feature-Vereinigung im Binary) – entspricht JSON.stringify.
    const artifactId = `sig_${sha256Hex(`${requestId}|${actorId}|${now}|${JSON.stringify(signers)}`)}`;
    const completed = status === 'completed';
    payload.status = status;
    if (completed) payload.signed_artifact_id = artifactId;
    await upsertRecord(tx, 'signature_requests', requestId, now, payload);
    return { status, artifactId: completed ? artifactId : null };
  });
  await recordAtsGovernanceEvent(ctx, command, atsActorValue(command), 'business_os.ats.signature_signed', {
    request_id: requestId, signer_id: signerId, status: out.status, signed_by_actor_id: actorId, signed_artifact_id: out.artifactId,
  }, now);
  return { ok: true, request_id: requestId, status: out.status, signed_artifact_id: out.artifactId };
}
