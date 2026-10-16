// invoices.invoice.post – Port aus invoices.rs invoices_invoice_post_stub :556 -> post_invoice_in_conn :569.
// Rust-Name "_stub", aber volle Logik: Nummernkreis reservieren (RE-/ER-/GS-/EG-/RT-<Jahr>-NNNN),
// Summen aus den Zeilen, ausgeglichene Buchung (Haupt-/Steuer-/Personenkonto), Status posted.
// Atomar (ctx.transaction statt SAVEPOINT), idempotent ueber die Buchung zur Rechnung.
import { commandIdOf, ensureModule, postInvoice, recordRequirement } from './_lib/invoices.js';

export default async function handle(command, ctx) {
  ensureModule(command);
  const invoiceId = recordRequirement(command, ['invoice_id', 'id']);
  return postInvoice(ctx, invoiceId, commandIdOf(command), ctx.now());
}
