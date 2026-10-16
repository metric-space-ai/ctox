// kundenpipeline.triage.failed – Rückruf des Kerns, wenn die Triage-Aufgabe fehlschlägt oder abgebrochen wird
// (§16.30 Nr. 3, on_failed; Abbruch mit status 'cancelled'). Rust kannte keinen Fehlerpfad: der Vorgang blieb still
// „zugeordnet“. Hier hält die App den Ausgang am Vorgang fest (triage_fehler + Audit), Status bleibt „zugeordnet“,
// damit der Owner weiter entscheiden kann. Idempotent: gleiche task_id oder fremder Auftrag → keine Wirkung.
import { COL_VORGAENGE, failureMessage, fail, loadAnyRecord, pushAudit, upsert } from './_lib/hub.js';

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  if (typeof payload.task_id !== 'string' || !payload.task_id) fail('invalid', 'task_id is required');
  const task = await ctx.cap.agent.task.get({ task_id: payload.task_id });
  const vorgangId = task?.payload?.vorgang_id;
  if (typeof vorgangId !== 'string' || !vorgangId) fail('not_found', `triage task ${payload.task_id} carries no vorgang_id`);
  const vorgang = await loadAnyRecord(ctx, COL_VORGAENGE, vorgangId);
  if (!vorgang) fail('not_found', `unknown Vorgang \`${vorgangId}\``);
  const status = payload.status === 'cancelled' ? 'cancelled' : 'failed';
  if (vorgang.status !== 'zugeordnet' || vorgang.triage_fehler?.task_id === payload.task_id
    || (vorgang.triage_task_id && vorgang.triage_task_id !== payload.task_id)) {
    return { ok: false, vorgang_id: vorgangId, status, applied: false };
  }
  const now = ctx.now();
  const message = failureMessage(payload);
  vorgang.triage_fehler = { task_id: payload.task_id, status, message, zeit_ms: now };
  vorgang.updated_at_ms = now;
  pushAudit(vorgang, now, status === 'cancelled' ? 'triage:abgebrochen' : 'triage:fehlgeschlagen', 'triage-agent', 'system');
  await upsert(ctx, COL_VORGAENGE, vorgang);
  return { ok: false, vorgang_id: vorgangId, status, error: message, applied: true };
}
