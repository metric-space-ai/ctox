// Origin: CTOX
// License: Apache-2.0
// ats.placement.create – ref: store_ats_commands.rs:475-615 (+ :289-325 Typ-/AÜG-Helfer) @ 4794af03d
// §5.8/§9.2 Einsatzbereitschafts-Gate: Pflicht-Nachweise = required_types ∪ (bei AÜG) Konfig
// CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS; AÜG mit leerer Menge → abgelehnt (fail closed). Bei Erfolg Placement
// + Honorar-Rechnungsentwurf inv_placement_<id> (accounting_invoices, App invoices).
import {
  asF64, asI64, atsActorValue, atsCanonicalInvoice, atsInvoiceLine, blockerJson, evaluateDeploymentReadiness, fail,
  firstStringField, loadByField, payloadOf, recordAtsGovernanceEvent, runtimeConfig, upsertRecord,
} from './_ats.js';

/** ref: :291 placement_type_requires_deployment_gate */
export function placementTypeRequiresDeploymentGate(placementType) {
  return ['arbeitnehmerueberlassung', 'arbeitnehmerüberlassung', 'aue', 'anü', 'anue', 'temp', 'temp_staffing',
    'zeitarbeit', 'ueberlassung', 'überlassung'].includes(asciiLower(placementType.trim()));
}
// Rust to_ascii_lowercase: nur A–Z werden klein (Ü bleibt Ü).
const asciiLower = (value) => value.replace(/[A-Z]/g, (ch) => ch.toLowerCase());

/** ref: :313 aue_mandatory_required_types */
export async function aueMandatoryRequiredTypes(ctx) {
  const raw = await runtimeConfig(ctx, 'CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS');
  if (raw === undefined) return [];
  return raw.split(/[,;\n\t ]/).map((s) => s.trim()).filter((s) => s !== '');
}

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const candidateId = firstStringField(p, ['candidate_id']);
  if (candidateId === undefined) fail('invalid', 'ats.placement.create requires candidate_id');
  const now = ctx.now();
  const actor = atsActorValue(command);
  const placementType = firstStringField(p, ['placement_type']) ?? '';
  const isAue = placementTypeRequiresDeploymentGate(placementType);
  const required = Array.isArray(p.required_types) ? p.required_types.filter((v) => typeof v === 'string') : [];
  if (isAue) {
    for (const ty of await aueMandatoryRequiredTypes(ctx)) if (!required.includes(ty)) required.push(ty);
    if (required.length === 0) {
      await recordAtsGovernanceEvent(ctx, command, actor, 'business_os.ats.placement_denied', {
        candidate_id: candidateId, decision: 'denied', reason_codes: ['aue_required_credentials_unconfigured'],
      }, now);
      return { ok: true, allowed: false, blockers: [{ reason: 'aue_required_credentials_unconfigured' }] };
    }
  }
  const outcome = await ctx.transaction(async (tx) => {
    if (required.length > 0) {
      const credentials = await loadByField(tx, 'business_credentials', 'subject_id', candidateId);
      const readiness = evaluateDeploymentReadiness(credentials, required, now);
      if (!readiness.ready) return { denied: true, blockers: blockerJson(readiness.blockers) };
    }
    const clientAccountId = firstStringField(p, ['client_account_id']) ?? '';
    const fee = asF64(p.fee) ?? 0;
    const id = `plac_${now}`;
    const invId = `inv_placement_${id}`;
    await upsertRecord(tx, 'placements', id, now, {
      id,
      candidate_id: candidateId,
      vacancy_id: firstStringField(p, ['vacancy_id']) ?? '',
      client_account_id: clientAccountId,
      fee_invoice_id: invId,
      offer_id: firstStringField(p, ['offer_id']) ?? '',
      start_ms: asI64(p.start_ms) ?? now,
      guarantee_days: asI64(p.guarantee_days) ?? 90,
      fee,
      status: 'confirmed',
      created_at_ms: now,
      updated_at_ms: now,
    });
    const invoice = atsCanonicalInvoice(invId, 'sale_out', clientAccountId,
      [atsInvoiceLine(1, 'Vermittlungshonorar', fee, 0.19)], now, 'source_placement_id', id);
    await upsertRecord(tx, 'accounting_invoices', invId, now, invoice);
    return { denied: false, id, invId, clientAccountId };
  });
  if (outcome.denied) {
    await recordAtsGovernanceEvent(ctx, command, actor, 'business_os.ats.placement_denied', {
      candidate_id: candidateId, decision: 'denied', reason_codes: ['deployment_not_ready'], blockers: outcome.blockers,
    }, now);
    return { ok: true, allowed: false, blockers: outcome.blockers };
  }
  await recordAtsGovernanceEvent(ctx, command, actor, 'business_os.ats.placement_created', {
    candidate_id: candidateId, client_account_id: outcome.clientAccountId, placement_id: outcome.id,
    fee_invoice_id: outcome.invId, decision: 'allowed',
  }, now);
  return { ok: true, allowed: true, placement_id: outcome.id, fee_invoice_id: outcome.invId };
}
