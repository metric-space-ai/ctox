// invoices.invoice.assign_payment_terms – Port aus invoices.rs invoices_invoice_assign_payment_terms_stub :1378.
// ECHTER STUB (treu übernommen): Rust bricht mit „not yet implemented“ ab. Keine Fachlogik erfunden;
// Fehlercode `unavailable`, Meldung wörtlich.
import { ensureModule, notImplemented } from './_lib/invoices.js';

export const STUB = true;

export default async function handle(command) {
  ensureModule(command);
  notImplemented('invoices.invoice.assign_payment_terms is not yet implemented (planned for Phase 7+); the payment terms are set in the invoice draft and cannot be reassigned post-post');
}
