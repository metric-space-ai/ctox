// invoices.payment.allocate – Port aus invoices.rs invoices_payment_allocate_stub :1494.
// Rust-Name "_stub", aber mit Logik: Zahlung (+Skonto) einer gebuchten Rechnung zuordnen,
// idempotent je (payment_id, invoice_id), Ueberzahlung abgelehnt, Status partially_paid/paid.
import { COL, asI64, commandIdOf, ensureModule, fail, loadInvoice, loadRecord, recordRequirement, str, upsertInvoice, upsertRecord } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const invoiceId = recordRequirement(command, ['invoice_id', 'id']);
  const payload = command.payload ?? {};
  const paymentId = str(payload.payment_id) ?? null;
  const amountCents = asI64(payload.amount_cents) ?? 0;
  const skontoCents = asI64(payload.skonto_cents) ?? 0;
  if (!(amountCents > 0 || skontoCents > 0)) fail('invalid', 'invoices.payment.allocate requires amount_cents or skonto_cents > 0');
  const commandId = commandIdOf(command) || `alloc_${invoiceId}_${ctx.now()}`;
  const allocationId = `alloc_${paymentId ?? `no_payment_${commandId}`}_${invoiceId}`;
  return ctx.transaction(async (tx) => {
    const existing = await loadRecord(tx, COL.allocations, allocationId);
    if (existing) {
      return { ok: true, idempotent: true, allocation: existing, invoice: (await loadInvoice(tx, invoiceId)) ?? null };
    }
    const invoice = await loadInvoice(tx, invoiceId);
    if (!invoice) fail('not_found', `invoice ${invoiceId} not found`);
    const state = str(invoice.state) ?? 'draft';
    if (!['posted', 'partially_paid', 'paid', 'overdue'].includes(state)) {
      fail('conflict', `invoices.payment.allocate requires posted/overdue invoice (current: ${state})`);
    }
    const totalCents = asI64(invoice.total_cents) ?? 0;
    const prevPaid = asI64(invoice.paid_cents) ?? 0;
    const effective = amountCents + skontoCents;
    const newPaid = prevPaid + effective;
    if (newPaid > totalCents) {
      fail('conflict', `allocation exceeds open invoice balance: total=${totalCents} paid=${prevPaid} applied=${effective}`);
    }
    const now = ctx.now();
    invoice.paid_cents = newPaid;
    invoice.open_cents = totalCents - newPaid;
    invoice.state = newPaid >= totalCents ? 'paid' : 'partially_paid';
    invoice.state_changed_at_ms = now;
    invoice.state_changed_by_command_id = commandId;
    invoice.updated_at_ms = now;
    await upsertInvoice(tx, invoiceId, invoice, now);
    const allocation = {
      id: allocationId,
      payment_id: paymentId,
      invoice_id: invoiceId,
      allocated_cents: amountCents,
      skonto_cents: skontoCents,
      note: null,
      allocated_at_ms: now,
      allocated_by_command_id: commandId,
      updated_at_ms: now,
    };
    await upsertRecord(tx, COL.allocations, allocationId, now, allocation);
    return { ok: true, invoice, allocation };
  });
}
