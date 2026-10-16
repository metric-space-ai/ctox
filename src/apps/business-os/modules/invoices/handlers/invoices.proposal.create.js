// invoices.proposal.create – Port aus invoices.rs invoices_proposal_create_stub :1857.
// ECHTER STUB (treu übernommen): Rust bricht mit „not yet implemented“ ab. Keine Fachlogik erfunden;
// Fehlercode `unavailable`, Meldung wörtlich.
import { ensureModule, notImplemented } from './_lib/invoices.js';

export const STUB = true;

export default async function handle(command) {
  ensureModule(command);
  notImplemented('invoices.proposal.create is not yet implemented (planned for Phase 7+); AI proposals currently surface in the inspector without a persisted command');
}
