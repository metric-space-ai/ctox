// Origin: CTOX
// License: Apache-2.0
// ats.placement.early_leave – ref: store_ats_commands.rs:1055-1150 @ 4794af03d (§5.11 Garantie/Frühausstieg)
// Austritt innerhalb der Garantiefrist → anteilige Gutschrift cn_earlyleave_<placement> (credit_note_out,
// credit_note_for_id = Honorarrechnung). Placement → status early_leave, left_at_ms.
import {
  asF64, asI64, atsActorValue, atsCanonicalInvoice, atsInvoiceLine, fail, firstStringField, payloadOf,
  recordAtsGovernanceEvent, roundHalfAwayFromZero, upsertRecord,
} from './_ats.js';

const DAY_MS = 86_400_000;

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const placementId = firstStringField(p, ['placement_id']);
  if (placementId === undefined) fail('invalid', 'ats.placement.early_leave requires placement_id');
  const now = ctx.now();
  const leftAtMs = asI64(p.left_at_ms) ?? now;
  const out = await ctx.transaction(async (tx) => {
    const existing = await tx.get('placements', placementId);
    if (!existing) fail('not_found', 'placement not found');
    const placement = JSON.parse(JSON.stringify(existing));
    const start = asI64(placement.start_ms) ?? 0;
    const days = asI64(placement.guarantee_days) ?? 0;
    const fee = asF64(placement.fee) ?? 0;
    const clientAccountId = typeof placement.client_account_id === 'string' ? placement.client_account_id : '';
    const within = start > 0 && days > 0 && leftAtMs < start + days * DAY_MS;
    let clawback = 0;
    let creditNoteId = '';
    if (within) {
      // Rust: i64-Division (trunc) und clamp(0, days)
      const served = Math.min(Math.max(Math.trunc((leftAtMs - start) / DAY_MS), 0), days);
      const remainingRatio = (days - served) / days;
      clawback = roundHalfAwayFromZero(fee * remainingRatio * 100) / 100;
      creditNoteId = `cn_earlyleave_${placementId}`;
      const feeInvoiceId = typeof placement.fee_invoice_id === 'string' ? placement.fee_invoice_id : '';
      const credit = atsCanonicalInvoice(creditNoteId, 'credit_note_out', clientAccountId,
        [atsInvoiceLine(1, 'Anteilige Gutschrift (Garantie/Frühausstieg)', clawback, 0.19)], now, 'source_placement_id', placementId);
      credit.credit_note_for_id = feeInvoiceId;
      await upsertRecord(tx, 'accounting_invoices', creditNoteId, now, credit);
    }
    placement.status = 'early_leave';
    placement.left_at_ms = leftAtMs;
    placement.updated_at_ms = now;
    await upsertRecord(tx, 'placements', placementId, now, placement);
    return { within, clawback, creditNoteId };
  });
  await recordAtsGovernanceEvent(ctx, command, atsActorValue(command), 'business_os.ats.placement_early_leave', {
    placement_id: placementId, within_guarantee: out.within, clawback: out.clawback, credit_note_id: out.creditNoteId,
  }, now);
  return { ok: true, placement_id: placementId, within_guarantee: out.within, clawback: out.clawback, credit_note_id: out.creditNoteId };
}
