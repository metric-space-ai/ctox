// ctox.queue.capacity – Port von harness_cockpit.rs:136-147 (4794af03d).
// Konfiguration der Worker-Kapazität ist Kern-Fähigkeit
// (service::configure_queue_worker_capacity).
import { invalid } from './_lib/errors.js';
import { runCockpitControl } from './_lib/cockpit.js';

export default async function handle(command, ctx) {
  return runCockpitControl(command, ctx, async (payload) => {
    const workers = payload.workers;
    // Rust: Value::as_u64 – nur nichtnegative Ganzzahlen.
    if (!Number.isInteger(workers) || workers < 0) throw invalid('workers must be an integer from 1 to 8');
    if (workers < 1 || workers > 8) throw invalid('workers must be 1..8');
    const capacity = await ctx.cap.agent.queue.control({ action: 'capacity', value: workers });
    return { ...(capacity && typeof capacity === 'object' ? capacity : {}), ok: true };
  });
}
