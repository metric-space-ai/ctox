// invoices.invoice.update – Port aus invoices.rs invoices_invoice_update_stub :486.
// Rust-Name "_stub", aber mit Logik: nur im Entwurf; Payload-Felder (ausser invoice_id/id)
// werden ungeprueft eingemischt, danach Mindestvalidierung, search_text neu.
import { ensureModule, fail, loadInvoice, recordRequirement, searchTextMap, str, upsertInvoice, validateInvoiceForCommand } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const invoiceId = recordRequirement(command, ['invoice_id', 'id']);
  const existing = await loadInvoice(ctx, invoiceId);
  if (!existing) fail('not_found', `invoice ${invoiceId} not found`);
  const state = str(existing.state) ?? 'draft';
  if (state !== 'draft') fail('conflict', `invoices.invoice.update is only allowed in state=draft (current: ${state})`);
  const payload = command.payload && typeof command.payload === 'object' && !Array.isArray(command.payload) ? command.payload : {};
  for (const [key, value] of Object.entries(payload)) {
    if (key === 'invoice_id' || key === 'id') continue;
    existing[key] = value; // Befund I1: auch state, invoice_number, total_cents … sind hier setzbar (wie Rust)
  }
  validateInvoiceForCommand(existing, false);
  const now = ctx.now();
  existing.updated_at_ms = now;
  existing.search_text = searchTextMap(existing);
  await upsertInvoice(ctx, invoiceId, existing, now);
  return { ok: true, invoice: existing };
}
