// invoices.import.from_outbound – Port aus invoices.rs invoices_import_from_outbound_stub :1849.
// ECHTER STUB (treu übernommen): Rust bricht mit „not yet implemented“ ab. Keine Fachlogik erfunden;
// Fehlercode `unavailable`, Meldung wörtlich.
import { ensureModule, notImplemented } from './_lib/invoices.js';

export const STUB = true;

export default async function handle(command) {
  ensureModule(command);
  notImplemented('invoices.import.from_outbound is not yet implemented (planned for Phase 7+); conversion from outbound_campaigns to an invoice draft goes through the normal customers.invoice.create_from_opportunity path');
}
