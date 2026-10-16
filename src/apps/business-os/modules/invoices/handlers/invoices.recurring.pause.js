// invoices.recurring.pause – Port aus invoices.rs invoices_recurring_pause_stub :1845.
// ECHTER STUB (treu übernommen): Rust bricht mit „not yet implemented“ ab. Keine Fachlogik erfunden;
// Fehlercode `unavailable`, Meldung wörtlich.
import { ensureModule, notImplemented } from './_lib/invoices.js';

export const STUB = true;

export default async function handle(command) {
  ensureModule(command);
  notImplemented('invoices.recurring.pause is not yet implemented (planned for Phase 7+)');
}
