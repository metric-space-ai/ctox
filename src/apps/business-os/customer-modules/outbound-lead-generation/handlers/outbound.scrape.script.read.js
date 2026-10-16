// outbound.scrape.script.read — aktuellen Skriptstand eines Scrape-Ziels aus der App-eigenen Registry lesen
// (Strang OLG725). Angeboten für die App outbound (commands_in from:['outbound'], §16.40): Kern §16.47 Nr. 4 lässt
// files.get nur für die eigene App zu; die Skripte gehören outbound-lead-generation. outbound liest den Text deshalb
// nicht mehr selbst (vorher modules/outbound/handlers/_lib/olg-registry.js readFileText über ctx.cap.files.get), sondern
// schickt diesen Befehl mit on_complete/on_failed; das Ergebnis dieses Handlers ist das Befehlsergebnis, das der Kern
// an den Rückruf der auslösenden App übergibt (§16.30 Nr. 3, §16.36).
// ref: store_outbound_commands.rs:1420-1481 outbound_registry_latest_script (portiert in _lib/scrape-registry.js latestScript).
import { fail, isObject } from './_lib/olg.js';
import { latestScript } from './_lib/scrape-registry.js';

/** Obergrenze wie Rust :1420 (150 KB); kleinere Werte darf der Aufrufer wünschen, größere nicht. */
export const SCRIPT_READ_MAX_BYTES = 150_000;

export default async function handle(command, ctx) {
  const payload = isObject(command.payload) ? command.payload : {};
  const targetKey = typeof payload.target_key === 'string' ? payload.target_key.trim() : '';
  if (!targetKey) fail('invalid', 'outbound.scrape.script.read: target_key fehlt');
  const wanted = Number.isInteger(payload.max_bytes) && payload.max_bytes > 0 ? payload.max_bytes : SCRIPT_READ_MAX_BYTES;
  const script = await latestScript(ctx, targetKey, Math.min(wanted, SCRIPT_READ_MAX_BYTES));
  return { ok: true, schema: 'ctox.outbound_lead_generation.scrape_script.v1', script };
}
