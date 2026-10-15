// K24-Probe – Befehls-Handler k24.tick.add (Vertrag §16.16: export default async function handle(command, ctx)).
// Fachlogik der App: Eingabe prüfen, einen Tick in die eigene Collection schreiben (ctx.put) und das Ereignis über das
// generische OS-Primitiv audit (ctx.cap.audit.event, CATALOG.md Nr. 13) protokollieren. Kein Netz, kein Dateisystem.

const COLLECTION = 'k24_ticks';
const MAX_LABEL = 200;

function fail(code, message) {
  // eslint-disable-next-line no-throw-literal
  throw { code, message };
}

export default async function handle(command, ctx) {
  const label = typeof command?.payload?.label === 'string' ? command.payload.label.trim() : '';
  if (!label) fail('invalid', 'k24.tick.add: label fehlt');
  if (label.length > MAX_LABEL) fail('invalid', `k24.tick.add: label länger als ${MAX_LABEL}`);
  const now = Number(ctx.now());
  const id = `cmd:${command.id}`;
  const doc = { id, label, source: 'command', command_id: String(command.id), weight: 1, created_at_ms: now };
  await ctx.put(COLLECTION, doc);
  await ctx.cap.audit.event({ collection: COLLECTION, record_id: id, event_type: 'k24.tick.added', payload: { label }, observed_at_ms: now });
  return { id };
}
