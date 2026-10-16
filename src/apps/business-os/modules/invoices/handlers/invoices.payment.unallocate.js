// invoices.payment.unallocate – Port aus invoices.rs invoices_payment_unallocate_stub :1607.
// Rust-Name "_stub", aber mit Logik: Zuordnung zuruecknehmen, paid/open/Status neu, Zuordnung -> Grabstein.
import { COL, asI64, commandIdOf, ensureModule, fail, loadInvoice, loadRecord, recordRequirement, str, tombstone, upsertInvoice } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const allocationId = recordRequirement(command, ['allocation_id', 'id']);
  return ctx.transaction(async (tx) => {
    const allocation = await loadRecord(tx, COL.allocations, allocationId);
    if (!allocation) fail('not_found', `allocation ${allocationId} not found`);
    const invoiceId = str(allocation.invoice_id);
    if (invoiceId === undefined) fail('invalid', 'allocation missing invoice_id');
    const allocated = asI64(allocation.allocated_cents) ?? 0;
    const skonto = asI64(allocation.skonto_cents) ?? 0;
    const invoice = await loadInvoice(tx, invoiceId);
    if (!invoice) fail('not_found', `invoice ${invoiceId} not found`);
    const now = ctx.now();
    const prevPaid = asI64(invoice.paid_cents) ?? 0;
    const newPaid = Math.max(prevPaid - allocated - skonto, 0);
    const totalCents = asI64(invoice.total_cents) ?? 0;
    invoice.paid_cents = newPaid;
    invoice.open_cents = totalCents - newPaid;
    invoice.state = newPaid <= 0 ? 'posted' : newPaid < totalCents ? 'partially_paid' : 'paid';
    invoice.state_changed_at_ms = now;
    invoice.state_changed_by_command_id = commandIdOf(command);
    invoice.updated_at_ms = now;
    await upsertInvoice(tx, invoiceId, invoice, now);
    // Rust setzt is_deleted/deleted_at_ms nur im Speicher und schreibt dann den Grabstein.
    await tombstone(tx, COL.allocations, allocationId);
    return { ok: true, invoice };
  });
}
