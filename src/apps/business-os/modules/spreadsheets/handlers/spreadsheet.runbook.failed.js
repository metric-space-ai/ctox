// Origin: CTOX
// License: Apache-2.0
// spreadsheet.runbook.failed – on_failed-Rückruf (Kern §16.30) der Seed-Runbooks spreadsheet.*, auch bei Abbruch
// (status 'cancelled'); kein UI-Sender. Vermerkt Grund am Lauf (spreadsheets.runbook_runs), idempotent je task_id.
import { settleSpreadsheetRunbook } from './_lib/automation.js';

export default async function handle(command, ctx) {
  return settleSpreadsheetRunbook(command, ctx, 'failed');
}
