// invoices.invoice.delete – Port aus invoices.rs invoices_invoice_delete_stub :523.
// Rust-Name "_stub", aber mit Logik: nur im Entwurf; Soft-Delete-Felder schreiben, dann Grabstein.
import { COL, ensureModule, fail, loadInvoice, recordRequirement, str, tombstone, upsertInvoice } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const invoiceId = recordRequirement(command, ['invoice_id', 'id']);
  return ctx.transaction(async (tx) => {
    const existing = await loadInvoice(tx, invoiceId);
    if (!existing) fail('not_found', `invoice ${invoiceId} not found`);
    const state = str(existing.state) ?? 'draft';
    if (state !== 'draft') fail('conflict', `invoices.invoice.delete is only allowed in state=draft (current: ${state})`);
    const now = ctx.now();
    existing.is_deleted = true;
    existing.deleted_at_ms = now;
    existing.updated_at_ms = now;
    await upsertInvoice(tx, invoiceId, existing, now);
    await tombstone(tx, COL.invoices, invoiceId);
    return { ok: true, invoice: existing };
  });
}
