// Origin: CTOX
// License: Apache-2.0
// research.systematic.report.failed – on_failed-Rückruf (Kern §16.30) der Aufgabe aus
// research.systematic.report.create, auch bei Abbruch (status 'cancelled'). Es entsteht kein Dokument; der Befehl
// selbst trägt Grund und Ausgangsbefehl, die App zeigt ihn in der Zustandszone (index.js watchAgentFailures).
// Schreibt nichts, also ohne Wirkung bei Doppelzustellung.
import { callbackError, sourceOfCallback } from './_lib/automation.js';

export default async function handle(command, ctx) {
  const { taskId, source } = await sourceOfCallback(command, ctx);
  const { status, message } = callbackError(command.payload);
  return { ok: false, status, error: message, task_id: taskId, command_id: source.id, title: source.payload?.title ?? null };
}
