// Origin: CTOX
// License: Apache-2.0
// ats.leistungsnachweis.signoff – ref: store_ats_commands.rs:709-870 @ 4794af03d
// Entleiher-Abzeichnung → Abrechnungs-Gate → Rechnungsentwurf (accounting_invoices, id inv_nachweis_<record>).
// Konfig CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE=1: Abzeichnung nur mit abgeschlossener signature_request
// (per signature_request_id oder jüngste mit document_id = Nachweis). Ziel-Collection payload.collection
// (Vorgabe planning_time_records), nur ATS-Collections. Kein Prüfspur-Ereignis (wie Rust).
import {
  asF64, atsCanonicalInvoice, checkedScaled, computeNachweisBilling, configFlag, ensureAtsCollectionAllowed,
  evaluateBillingGate, fail, firstStringField, isObject, payloadOf, upsertRecord,
} from './_ats.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const collection = typeof p.collection === 'string' ? p.collection : 'planning_time_records';
  ensureAtsCollectionAllowed(collection);
  const recordId = firstStringField(p, ['record_id', 'nachweis_id']);
  if (recordId === undefined) fail('invalid', 'ats.leistungsnachweis.signoff requires record_id');
  const requireSignature = await configFlag(ctx, 'CTOX_BUSINESS_OS_REQUIRE_ENTLEIHER_SIGNATURE');
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const existing = await tx.get(collection, recordId);
    const payload = existing ? JSON.parse(JSON.stringify(existing)) : { id: recordId };
    let signatureProven = true;
    if (requireSignature) {
      const sigId = firstStringField(p, ['signature_request_id']);
      let request = null;
      if (sigId !== undefined) {
        request = await tx.get('signature_requests', sigId);
      } else {
        const rows = await tx.read('signature_requests', { selector: { document_id: recordId }, sort: [{ updated_at_ms: 'desc' }], limit: 1 });
        request = rows[0] ?? null;
      }
      signatureProven = request?.status === 'completed';
    }
    if (signatureProven && isObject(payload)) {
      payload.entleiher_signed = true;
      payload.signed_at_ms = now;
    }
    const blockers = [...evaluateBillingGate(payload)];
    if (requireSignature && !signatureProven) blockers.push('entleiher_signature_proof_missing');
    const chargeRate = asF64(p.charge_rate) ?? asF64(payload.charge_rate) ?? 0;
    if (!(Number.isFinite(chargeRate) && chargeRate > 0)) blockers.push('missing_charge_rate');
    let invoiceId = '';
    let netTotal = 0;
    if (blockers.length === 0) {
      const entries = Array.isArray(payload.entries) ? payload.entries : [];
      const surcharge = p.surcharge_pct !== undefined ? p.surcharge_pct : (payload.surcharge_pct !== undefined ? payload.surcharge_pct : {});
      const billing = computeNachweisBilling(entries, chargeRate, surcharge);
      netTotal = billing.net_total;
      if (netTotal > 0) {
        invoiceId = `inv_nachweis_${recordId}`;
        const accountId = (typeof payload.entleiher_account_id === 'string' ? payload.entleiher_account_id : undefined)
          ?? firstStringField(p, ['entleiher_account_id', 'client_account_id']) ?? '';
        const lines = billing.lines.map((line, i) => ({
          position: i + 1,
          description: `Arbeitsstunden (${line.category})`,
          quantity: checkedScaled(line.hours, 1000),
          unit_price_cents: checkedScaled(line.rate, 100),
          tax_rate: 0.19,
          account_code: '8400',
        }));
        const invoice = atsCanonicalInvoice(invoiceId, 'sale_out', accountId, lines, now, 'source_nachweis_id', recordId);
        await upsertRecord(tx, 'accounting_invoices', invoiceId, now, invoice);
      }
    }
    const billingReleased = invoiceId !== '';
    if (isObject(payload)) payload.billing_released = billingReleased;
    await upsertRecord(tx, collection, recordId, now, payload);
    return { ok: true, record_id: recordId, billing_released: billingReleased, blockers, invoice_id: invoiceId, net_total: netTotal };
  });
}
