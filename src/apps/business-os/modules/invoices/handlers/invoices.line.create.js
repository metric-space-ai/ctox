// invoices.line.create – Port aus invoices.rs invoices_line_create :1423. Zeile anhaengen (nur Entwurf).
import { ensureModule, fail, loadDraftInvoiceLines, recordRequirement, saveDraftInvoiceLines } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const invoiceId = recordRequirement(command, ['invoice_id']);
  const payload = command.payload ?? {};
  if (!Object.prototype.hasOwnProperty.call(payload, 'line')) fail('invalid', "invoices.line.create requires a 'line' object");
  const line = payload.line;
  if (line === null || typeof line !== 'object' || Array.isArray(line)) fail('invalid', "invoices.line.create 'line' must be an object");
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const { invoice, lines } = await loadDraftInvoiceLines(tx, invoiceId);
    lines.push(line);
    return { ok: true, invoice: await saveDraftInvoiceLines(tx, invoiceId, invoice, lines, now) };
  });
}
