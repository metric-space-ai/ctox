// invoices.recurring.run – Port aus invoices.rs invoices_recurring_run_stub :1838.
// ECHTER STUB (treu übernommen): Rust bricht mit „not yet implemented“ ab. Keine Fachlogik erfunden;
// Fehlercode `unavailable`, Meldung wörtlich.
import { ensureModule, notImplemented } from './_lib/invoices.js';

export const STUB = true;

export default async function handle(command) {
  ensureModule(command);
  notImplemented('invoices.recurring.run is not yet implemented (planned for Phase 7+); the external scheduler drives periodic invoice generation');
}
