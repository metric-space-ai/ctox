// research.systematic.run.failed – on_failed-Rückruf des Recherche-Auftrags (Kern §16.30 Nr. 3), kein UI-Sender.
// payload {task_id, status:'failed'|'cancelled', error?}. Markiert Lauf (research_runs) und Aufgabe (research_tasks);
// der Zustand erscheint live über die Collections. Idempotent bei Doppelzustellung.
// ref: store.rs:22820-22943 (Fehlerpersistenz des nativen Abschlusses)
import { callbackOf, persistRunFailure } from './_lib/callback.js';

export default async function handle(command, ctx) {
  return persistRunFailure(ctx, callbackOf(command));
}
