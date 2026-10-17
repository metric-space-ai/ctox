// kundenpipeline.auftrag.complete – Rückruf des Kerns nach Abschluss des delegierten Kundenauftrags (§16.30 Nr. 3,
// on_complete aus kundenpipeline.delegate): payload {task_id, status:'completed', result, workspace_refs?}.
// Rust hatte keinen Rückweg: der Agent sollte per CLI kundenpipeline.triage.write mit leerem triage:{} senden, was
// triage_json überschrieb und den Vorgang auf „triagiert“ zurücksetzte (Befund F-automatisierung #9).
// Hier: Ergebnis-Zusammenfassung in run_json.zusammenfassung, Status „ergebnisVorliegend“ und eine Karte
// „mailfreigabe“ – genau das, was kundenpipeline.decision.answer (runAcceptanceFollowup, Rust :646) und
// kundenpipeline.mail.send (art 'ergebnis', Rust :825) schon erwarten. Idempotent: run_json.task_key muss die
// task_id sein, ein zweites Eintreffen (run_json.status gesetzt) bleibt ohne Wirkung.
import {
  COL_ENTSCHEIDUNGEN, COL_VORGAENGE, WRAP_WIDTH, decisionRecord, deterministicId, fail, kurz, loadAnyRecord,
  pushAudit, resultText, senderAddressOf, upsert, workspaceDecisionUserIds, wrapText,
} from './_lib/hub.js';

export const SUMMARY_MAX_CHARS = 8_000;

export async function loadRunVorgang(ctx, payload) {
  if (typeof payload.task_id !== 'string' || !payload.task_id) fail('invalid', 'task_id is required');
  const task = await ctx.cap.agent.task.get({ task_id: payload.task_id });
  const vorgangId = task?.payload?.vorgang_id;
  if (typeof vorgangId !== 'string' || !vorgangId) fail('not_found', `auftrag task ${payload.task_id} carries no vorgang_id`);
  const vorgang = await loadAnyRecord(ctx, COL_VORGAENGE, vorgangId);
  if (!vorgang) fail('not_found', `unknown Vorgang \`${vorgangId}\``);
  const current = vorgang.run_json && typeof vorgang.run_json === 'object' ? vorgang.run_json : null;
  const pending = Boolean(current) && current.task_key === payload.task_id && current.status === undefined;
  return { vorgangId, vorgang, pending };
}

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  if (payload.status !== 'completed') fail('invalid', `auftrag callback with status ${payload.status} (on_failed: kundenpipeline.auftrag.failed)`);
  const { vorgangId, vorgang, pending } = await loadRunVorgang(ctx, payload);
  if (!pending) return { ok: true, vorgang_id: vorgangId, applied: false };
  const now = ctx.now();
  const zusammenfassung = kurz(resultText(payload.result), SUMMARY_MAX_CHARS);
  vorgang.run_json = {
    ...vorgang.run_json,
    status: 'completed',
    zusammenfassung,
    workspace_refs: Array.isArray(payload.workspace_refs) ? payload.workspace_refs : [],
    abgeschlossen_ms: now,
  };
  vorgang.status = 'ergebnisVorliegend';
  vorgang.updated_at_ms = now;
  pushAudit(vorgang, now, 'auftrag:ergebnis', 'code-agent', 'system');
  await upsert(ctx, COL_VORGAENGE, vorgang);

  const zeilen = ['▸ ERGEBNIS', ...wrapText(zusammenfassung, WRAP_WIDTH)];
  const titel = typeof vorgang.kunde_name === 'string' && vorgang.kunde_name ? vorgang.kunde_name : senderAddressOf(vorgang);
  const decisionId = deterministicId('kpl-e-mail', vorgangId);
  const decision = decisionRecord(decisionId, vorgangId, 'mailfreigabe', kurz(titel, 40), zeilen, await workspaceDecisionUserIds(ctx), now);
  await upsert(ctx, COL_ENTSCHEIDUNGEN, decision);
  return { ok: true, vorgang_id: vorgangId, applied: true, decision_id: decisionId };
}
