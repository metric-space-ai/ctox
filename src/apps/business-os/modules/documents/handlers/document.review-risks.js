// Origin: CTOX
// License: Apache-2.0
// document.review-risks – Seed-Runbook „document.review-risks“ als Agentenaufgabe mit Rückruf (Strang SKL-B3, Kern §16.30); siehe _lib/document-runbook.js.
import { enqueueDocumentRunbook } from './_lib/document-runbook.js';

export default async function handle(command, ctx) {
  return enqueueDocumentRunbook(command, ctx);
}
