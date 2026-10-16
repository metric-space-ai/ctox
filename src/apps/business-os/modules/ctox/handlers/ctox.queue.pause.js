// ctox.queue.pause – Port von harness_cockpit.rs:118-135 (4794af03d).
// Der Wert liegt im Runtime-Store unter "queue.pause" (Admission der Kern-
// Warteschlange) → ctx.cap.agent.queue.control({action:'pause'}).
import { invalid } from './_lib/errors.js';
import { optionalText, runCockpitControl } from './_lib/cockpit.js';

export default async function handle(command, ctx) {
  return runCockpitControl(command, ctx, async (payload) => {
    if (typeof payload.paused !== 'boolean') throw invalid('paused must be a boolean');
    const reason = optionalText(payload, 'reason', 1000);
    const value = { paused: payload.paused, reason: payload.paused ? reason : null };
    await ctx.cap.agent.queue.control({ action: 'pause', value });
    return { ok: true, pause: value };
  });
}
