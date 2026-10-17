// invoices.line.update – Port aus invoices.rs invoices_line_update :1442. Zeile line_index patchen
// (Objekt in Objekt einmischen, sonst ersetzen), nur Entwurf.
import { asU64, ensureModule, fail, loadDraftInvoiceLines, recordRequirement, saveDraftInvoiceLines } from './_lib/invoices.js';

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

export default async function handle(command, ctx) {
  ensureModule(command);
  const invoiceId = recordRequirement(command, ['invoice_id']);
  const payload = command.payload ?? {};
  const index = asU64(payload.line_index);
  if (index === undefined) fail('invalid', "invoices.line.update requires a 'line_index'");
  if (!Object.prototype.hasOwnProperty.call(payload, 'line')) fail('invalid', "invoices.line.update requires a 'line' object");
  const patch = payload.line;
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const { invoice, lines } = await loadDraftInvoiceLines(tx, invoiceId);
    if (index >= lines.length) fail('invalid', `line_index ${index} out of range (${lines.length} lines)`);
    lines[index] = isObject(lines[index]) && isObject(patch) ? { ...lines[index], ...patch } : patch;
    return { ok: true, invoice: await saveDraftInvoiceLines(tx, invoiceId, invoice, lines, now) };
  });
}
