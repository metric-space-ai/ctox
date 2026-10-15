// K24-Probe – Rückruf on_complete des Agentenlaufs (Befehl {task_id, status, result}, §16.30 Nr. 3).
import { settle } from './_runs.js';

export default async function handle(command, ctx) {
  return settle(command, ctx, 'done');
}
