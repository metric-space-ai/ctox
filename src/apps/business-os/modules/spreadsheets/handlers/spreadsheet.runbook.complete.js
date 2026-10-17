// Origin: CTOX
// License: Apache-2.0
// spreadsheet.runbook.complete – on_complete-Rückruf (Kern §16.30) der Seed-Runbooks spreadsheet.*; kein UI-Sender.
// Schreibt das Ergebnis an spreadsheets.runbook_runs[<runbook>] (live bei allen Clients); idempotent je task_id.
import { settleSpreadsheetRunbook } from './_lib/automation.js';

export default async function handle(command, ctx) {
  return settleSpreadsheetRunbook(command, ctx, 'completed');
}
