// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.writeback.complete – on_complete-Befehl des agent.task-Rückschriebs (Kern §16.30), kein UI-Sender.
// payload {task_id, status, result, error?, workspace?}; idempotent bei Doppelzustellung (siehe _writeback.js).
import { completeWriteback } from './_writeback.js';

export default async function handle(command, ctx) {
  return completeWriteback(command, ctx);
}
