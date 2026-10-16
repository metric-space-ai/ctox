// invoices.invoice.create_credit_note – Port aus invoices.rs invoices_invoice_create_credit_note :1284.
// Entwurf einer §17-UStG-Gutschrift zu einer GEBUCHTEN Rechnung; deterministische ID cn_<original>
// (oder credit_note_id/new_invoice_id); kumulative Obergrenze: Summe aller Gutschriften <= Netto des Originals.
import {
  COL, asI64, buildCreditNoteDraft, ensureModule, fail, lineAmounts, loadInvoice, recordRequirement, str, upsertInvoice,
} from './_lib/invoices.js';

// ref: :1309 line_net – nur Netto (ohne Steuer) der Zeilen
const lineNet = (inv) => (Array.isArray(inv?.lines) ? inv.lines.reduce((sum, line) => sum + lineAmounts(line).net, 0) : 0);

export default async function handle(command, ctx) {
  ensureModule(command);
  const creditNoteForId = recordRequirement(command, ['credit_note_for_id', 'invoice_id', 'id']);
  const now = ctx.now();
  const payload = command.payload ?? {};
  // Rust: .get("credit_note_id").or_else(|| .get("new_invoice_id")).and_then(as_str) – ein vorhandener,
  // aber nicht-String credit_note_id verdeckt new_invoice_id. Treu nachgebildet.
  const rawNew = Object.prototype.hasOwnProperty.call(payload, 'credit_note_id') ? payload.credit_note_id : payload.new_invoice_id;
  const effectiveNewId = typeof rawNew === 'string' ? rawNew : `cn_${creditNoteForId}`;
  return ctx.transaction(async (tx) => {
    const existing = await loadInvoice(tx, effectiveNewId);
    if (existing) return { ok: true, idempotent: true, invoice: existing };
    const original = await loadInvoice(tx, creditNoteForId);
    if (!original) fail('not_found', `invoice ${creditNoteForId} not found`);
    const state = str(original.state) ?? 'draft';
    if (state !== 'posted') fail('conflict', `credit note requires a posted invoice to correct (state: ${state})`);
    const cn = buildCreditNoteDraft(original, effectiveNewId, command, now);
    const originalNet = asI64(original.subtotal_cents) ?? lineNet(original);
    const siblings = await tx.read(COL.invoices, { selector: { credit_note_for_id: creditNoteForId } });
    const existingNet = siblings.filter((d) => d.id !== effectiveNewId).reduce((sum, d) => sum + lineNet(d), 0);
    const newNet = lineNet(cn);
    if (existingNet + newNet > originalNet) {
      fail('conflict', `credit notes for ${creditNoteForId} would exceed the invoice net (${existingNet} + ${newNet} > ${originalNet})`);
    }
    await upsertInvoice(tx, effectiveNewId, cn, now);
    return { ok: true, invoice: cn };
  });
}
