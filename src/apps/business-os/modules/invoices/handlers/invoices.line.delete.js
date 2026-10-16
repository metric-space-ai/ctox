// invoices.line.delete – Port aus invoices.rs invoices_line_delete :1473. Zeile line_index entfernen, nur Entwurf.
import { asU64, ensureModule, fail, loadDraftInvoiceLines, recordRequirement, saveDraftInvoiceLines } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const invoiceId = recordRequirement(command, ['invoice_id']);
  const index = asU64(command.payload?.line_index);
  if (index === undefined) fail('invalid', "invoices.line.delete requires a 'line_index'");
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const { invoice, lines } = await loadDraftInvoiceLines(tx, invoiceId);
    if (index >= lines.length) fail('invalid', `line_index ${index} out of range (${lines.length} lines)`);
    lines.splice(index, 1);
    return { ok: true, invoice: await saveDraftInvoiceLines(tx, invoiceId, invoice, lines, now) };
  });
}
