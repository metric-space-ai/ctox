// invoices.recurring.update – Port aus invoices.rs invoices_recurring_update_stub :1834.
// ECHTER STUB (treu übernommen): Rust bricht mit „not yet implemented“ ab. Keine Fachlogik erfunden;
// Fehlercode `unavailable`, Meldung wörtlich.
import { ensureModule, notImplemented } from './_lib/invoices.js';

export const STUB = true;

export default async function handle(command) {
  ensureModule(command);
  notImplemented('invoices.recurring.update is not yet implemented (planned for Phase 7+)');
}
