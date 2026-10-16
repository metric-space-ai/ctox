// ctox.queue.abort_turn – Port von harness_cockpit.rs:148-151 (4794af03d).
// Nativ fest „unsupported“; keine Fähigkeit nötig. task_id ist Pflicht.
import { runCockpitControl } from './_lib/cockpit.js';

export default async function handle(command, ctx) {
  return runCockpitControl(command, ctx, async () => ({
    ok: false,
    status: 'unsupported',
    code: 'unsupported',
    reason:
      'The worker owns the active session. Safe abort requires a task-scoped interrupt acknowledgement and an atomic aborted-attempt/blocked-lease transition. Use queue.pause to stop new admission.',
  }));
}
