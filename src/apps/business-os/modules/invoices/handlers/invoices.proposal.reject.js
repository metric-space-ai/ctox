// invoices.proposal.reject – Port aus invoices.rs invoices_proposal_reject_stub :1868.
// ECHTER STUB (treu übernommen): Rust bricht mit „not yet implemented“ ab. Keine Fachlogik erfunden;
// Fehlercode `unavailable`, Meldung wörtlich.
import { ensureModule, notImplemented } from './_lib/invoices.js';

export const STUB = true;

export default async function handle(command) {
  ensureModule(command);
  notImplemented('invoices.proposal.reject is not yet implemented (planned for Phase 7+)');
}
