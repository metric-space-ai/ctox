// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.writeback.failed – on_failed-Befehl des agent.task-Rückschriebs (Kern §16.30), kein UI-Sender.
// payload {task_id, status, result, error?, workspace?}; idempotent bei Doppelzustellung (siehe _writeback.js).
import { failWriteback } from './_writeback.js';

export default async function handle(command, ctx) {
  return failWriteback(command, ctx);
}
