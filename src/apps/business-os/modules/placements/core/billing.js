// core/billing.js — pure placement-fee billing bridge. No DOM, no RxDB.
//
// Baukasten note: turns a lifecycle outcome (a confirmed placement / an early
// leave) into a draft billing document the invoices module posts. Generic
// "lifecycle event -> draft invoice line"; recruiting maps it to the placement
// fee + pro-rata guarantee clawback. Amounts are passed in (computed by the
// lifecycle model) — no rates hardcoded here.

/**
 * Draft an outgoing placement-fee invoice for a confirmed placement.
 * @param {{id: string, client_account_id: string, candidate_name?: string, fee: number}} placement
 * @param {{currency?: string, taxRate?: number, atMs?: number}} [opts]
 */
export function draftPlacementFeeInvoice(placement, { currency = 'EUR', taxRate = 0.19, atMs = 0 } = {}) {
  const fee = Number(placement?.fee) || 0;
  return {
    kind: 'invoice_draft',
    invoice_type: 'sale_out',
    account_id: placement?.client_account_id || '',
    source_placement_id: placement?.id || '',
    currency,
    lines: [
      {
        position: 1,
        description: `Vermittlungshonorar${placement?.candidate_name ? ` — ${placement.candidate_name}` : ''}`,
        quantity: 1,
        unit_price: fee,
        tax_rate: taxRate,
      },
    ],
    net_total: round2(fee),
    created_at_ms: atMs,
  };
}

/**
 * Draft a credit note for a pro-rata guarantee clawback when a placement leaves
 * early within the guarantee window.
 * @param {{id: string, client_account_id: string}} placement
 * @param {number} clawbackAmount
 * @param {{currency?: string, taxRate?: number, atMs?: number}} [opts]
 */
export function draftClawbackCreditNote(placement, clawbackAmount, { currency = 'EUR', taxRate = 0.19, atMs = 0 } = {}) {
  const amount = Number(clawbackAmount) || 0;
  return {
    kind: 'credit_note_draft',
    invoice_type: 'credit_note_out',
    account_id: placement?.client_account_id || '',
    source_placement_id: placement?.id || '',
    currency,
    lines: [
      { position: 1, description: 'Anteilige Gutschrift (Garantie/Frühausstieg)', quantity: 1, unit_price: amount, tax_rate: taxRate },
    ],
    net_total: round2(amount),
    created_at_ms: atMs,
  };
}

function round2(value) {
  return Math.round((Number(value) || 0) * 100) / 100;
}
