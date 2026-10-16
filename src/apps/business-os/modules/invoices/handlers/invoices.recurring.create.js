// invoices.recurring.create – Port aus invoices.rs invoices_recurring_create_stub :1826.
// ECHTER STUB (treu übernommen): Rust bricht mit „not yet implemented“ ab. Keine Fachlogik erfunden;
// Fehlercode `unavailable`, Meldung wörtlich.
import { ensureModule, notImplemented } from './_lib/invoices.js';

export const STUB = true;

export default async function handle(command) {
  ensureModule(command);
  notImplemented('invoices.recurring.create is not yet implemented (planned for Phase 7+); recurring templates live in the invoice draft and are instantiated by an external scheduler that posts invoices via the normal create+post path');
}
