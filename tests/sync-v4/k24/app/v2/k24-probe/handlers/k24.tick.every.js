// K24-Probe – every-Trigger (Manifest triggers: {handler, every: 'PT1M'}, §16.20). Schreibt je Minute höchstens einen
// Tick (ID aus der Core-Uhr auf Minuten gerundet → idempotent bei Wiederholung unter der Executor-Fence).

const COLLECTION = 'k24_ticks';

export default async function handle(_command, ctx) {
  const now = Number(ctx.now());
  const minute = Math.floor(now / 60000);
  const id = `trigger:${minute}`;
  await ctx.put(COLLECTION, { id, label: `Trigger ${new Date(minute * 60000).toISOString().slice(11, 16)}`, source: 'trigger', weight: 1, created_at_ms: now });
  return { id };
}
