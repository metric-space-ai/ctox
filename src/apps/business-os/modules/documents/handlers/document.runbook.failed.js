// Origin: CTOX
// License: Apache-2.0
// document.runbook.failed – on_failed-Rückruf (Kern §16.30) der Seed-Runbooks document.*, auch bei Abbruch
// (status 'cancelled'); kein UI-Sender. Vermerkt Grund am Lauf (documents.runbook_runs), idempotent je task_id.
import { settleDocumentRunbook } from './_lib/document-runbook.js';

export default async function handle(command, ctx) {
  return settleDocumentRunbook(command, ctx, 'failed');
}
