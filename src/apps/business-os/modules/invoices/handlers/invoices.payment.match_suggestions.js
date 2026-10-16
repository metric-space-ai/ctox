// invoices.payment.match_suggestions – Port aus invoices.rs invoices_payment_match_suggestions_stub :1667.
// ECHTER STUB (treu übernommen): liefert immer eine leere Vorschlagsliste. Keine Fachlogik erfunden.
import { ensureModule } from './_lib/invoices.js';

export default async function handle(command) {
  ensureModule(command);
  return { ok: true, stub: true, command: 'invoices.payment.match_suggestions', phase: 3, suggestions: [] };
}
