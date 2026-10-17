// invoices.invoice.create – Port aus invoices.rs invoices_invoice_create_stub :457.
// Rust-Name "_stub", aber mit Logik: idempotent je invoice_id, Entwurf aus dem Payload
// (currency fest EUR, Summenfelder 0, search_text), Mindestvalidierung, speichern.
import { ensureModule, loadInvoice, objectFromCommand, recordRequirement, upsertInvoice, validateInvoiceForCommand } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const invoiceId = recordRequirement(command, ['invoice_id', 'id']);
  const existing = await loadInvoice(ctx, invoiceId);
  if (existing) return { ok: true, idempotent: true, invoice: existing };
  const now = ctx.now();
  const invoice = objectFromCommand(command, invoiceId);
  invoice.id = invoiceId;
  invoice.state = 'draft';
  invoice.is_deleted = false;
  invoice.created_at_ms = now;
  invoice.updated_at_ms = now;
  validateInvoiceForCommand(invoice, false);
  await upsertInvoice(ctx, invoiceId, invoice, now);
  return { ok: true, invoice };
}
