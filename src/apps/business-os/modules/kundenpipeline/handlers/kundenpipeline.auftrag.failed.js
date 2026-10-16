// kundenpipeline.auftrag.failed – Rückruf des Kerns, wenn der delegierte Kundenauftrag fehlschlägt oder abgebrochen
// wird (§16.30 Nr. 3, on_failed; Abbruch mit status 'cancelled'). Hält den Ausgang in run_json fest
// (status, fehler, beendet_ms) und protokolliert ihn; der Vorgang bleibt „inArbeit“, der Owner entscheidet weiter.
// Idempotent wie kundenpipeline.auftrag.complete (run_json.task_key = task_id, run_json.status noch leer).
import { COL_VORGAENGE, failureMessage, pushAudit, upsert } from './_lib/hub.js';
import { loadRunVorgang } from './kundenpipeline.auftrag.complete.js';

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  const { vorgangId, vorgang, pending } = await loadRunVorgang(ctx, payload);
  const status = payload.status === 'cancelled' ? 'cancelled' : 'failed';
  if (!pending) return { ok: false, vorgang_id: vorgangId, status, applied: false };
  const now = ctx.now();
  const message = failureMessage(payload);
  vorgang.run_json = { ...vorgang.run_json, status, fehler: message, beendet_ms: now };
  vorgang.updated_at_ms = now;
  pushAudit(vorgang, now, status === 'cancelled' ? 'auftrag:abgebrochen' : 'auftrag:fehlgeschlagen', 'code-agent', 'system');
  await upsert(ctx, COL_VORGAENGE, vorgang);
  return { ok: false, vorgang_id: vorgangId, status, error: message, applied: true };
}
