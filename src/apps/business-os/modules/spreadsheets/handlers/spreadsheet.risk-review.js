// Origin: CTOX
// License: Apache-2.0
// spreadsheet.risk-review – Seed-Runbook als Agentenaufgabe mit Rückruf (Strang SKL-B3, Kern §16.30); siehe _lib/automation.js.
import { enqueueSpreadsheetRunbook } from './_lib/automation.js';

export default async function handle(command, ctx) {
  return enqueueSpreadsheetRunbook(command, ctx);
}
