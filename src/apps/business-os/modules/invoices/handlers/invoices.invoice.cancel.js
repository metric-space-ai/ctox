// invoices.invoice.cancel – Port aus invoices.rs invoices_invoice_cancel :1210 (§5.11 Storno).
// Entwurf: in place storniert + Grabstein. Gebucht: Storno-Gutschrift cn_<id>_storno aus dem Original
// neu aufbauen (ausser sie ist schon gebucht), buchen, Original cancelled + Verweis. Bereits storniert: idempotent.
import {
  COL, buildCreditNoteDraft, commandIdOf, ensureModule, fail, loadInvoice, postInvoice, recordRequirement, str, tombstone, upsertInvoice,
} from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const invoiceId = recordRequirement(command, ['invoice_id', 'id']);
  return ctx.transaction(async (tx) => {
    const invoice = await loadInvoice(tx, invoiceId);
    if (!invoice) fail('not_found', `invoice ${invoiceId} not found`);
    const state = str(invoice.state) ?? 'draft';
    const now = ctx.now();
    if (state === 'cancelled') return { ok: true, idempotent: true, invoice_id: invoiceId, state: 'cancelled' };
    if (state === 'draft') {
      invoice.state = 'cancelled';
      invoice.is_deleted = true;
      invoice.cancelled_at_ms = now;
      invoice.updated_at_ms = now;
      await upsertInvoice(tx, invoiceId, invoice, now);
      await tombstone(tx, COL.invoices, invoiceId);
      return { ok: true, invoice_id: invoiceId, state: 'cancelled' };
    }
    if (state === 'posted') {
      const cnId = `cn_${invoiceId}_storno`;
      const existingCn = await loadInvoice(tx, cnId);
      if (str(existingCn?.state) !== 'posted') {
        const cn = buildCreditNoteDraft(invoice, cnId, command, now);
        await upsertInvoice(tx, cnId, cn, now);
      }
      const stornoPosted = await postInvoice(tx, cnId, commandIdOf(command), now, { inTransaction: true });
      invoice.state = 'cancelled';
      invoice.cancelled_at_ms = now;
      invoice.storno_credit_note_id = cnId;
      invoice.updated_at_ms = now;
      await upsertInvoice(tx, invoiceId, invoice, now);
      return { ok: true, invoice_id: invoiceId, state: 'cancelled', storno_credit_note_id: cnId, storno_posted: stornoPosted };
    }
    fail('conflict', `cannot cancel invoice in state ${state} (only draft or posted)`);
  });
}
