// Origin: CTOX
// License: Apache-2.0
// document.runbook.complete – on_complete-Rückruf (Kern §16.30) der Seed-Runbooks document.*; kein UI-Sender.
// Schreibt das Ergebnis an documents.runbook_runs[<runbook>] (live bei allen Clients); idempotent je task_id.
import { settleDocumentRunbook } from './_lib/document-runbook.js';

export default async function handle(command, ctx) {
  return settleDocumentRunbook(command, ctx, 'completed');
}
