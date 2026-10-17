// kundenpipeline.triage.write – Port aus decision_hub.rs handle_triage_write :713.
// Triage-Vorschlag eintragen: triage_json + Status "triagiert" am Vorgang, dazu genau eine Triage-
// Entscheidungskarte (deterministische ID).
// Zwei Formen (Strang SKL-B4):
//   Rückruf (on_complete der Triage aus kundenpipeline.mail.inbound, §16.30 Nr. 3): payload {task_id, status,
//     result}; vorgang_id aus der Aufgabe (agent.task.get → payload.vorgang_id), Vorschlag = erstes JSON-Objekt der
//     Antwort (ref: store.rs:23796 extract_json_object; {triage:{…}} oder das Objekt selbst). Doppelzustellung bzw.
//     ein Vorgang, der nicht mehr auf diese Triage wartet (status ≠ zugeordnet), bleibt ohne Wirkung.
//   Direkt {vorgang_id, triage} (manuell). Der frühere CLI-Rückschrieb des Agenten entfällt.

import {
  COL_ENTSCHEIDUNGEN,
  COL_VORGAENGE,
  WRAP_WIDTH,
  cleanBodyOr,
  decisionRecord,
  deterministicId,
  extractJsonObject,
  fail,
  kurz,
  loadAnyRecord,
  pushAudit,
  resultText,
  senderAddressOf,
  upsert,
  workspaceDecisionUserIds,
  wrapText,
} from './_lib/hub.js';

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

/** Rückruf-Form: Aufgabe lesen, Antwort in einen Triage-Vorschlag übersetzen. */
async function fromCallback(ctx, payload) {
  const task = await ctx.cap.agent.task.get({ task_id: payload.task_id });
  const vorgangId = task?.payload?.vorgang_id;
  if (typeof vorgangId !== 'string' || !vorgangId) fail('not_found', `triage task ${payload.task_id} carries no vorgang_id`);
  const reply = extractJsonObject(resultText(payload.result));
  const triage = isObject(reply?.triage) ? reply.triage : reply;
  if (!isObject(triage)) fail('invalid', 'triage reply did not contain a JSON object');
  return { vorgangId, triage };
}

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  const callback = typeof payload.task_id === 'string' && payload.task_id !== '';
  if (callback && payload.status !== 'completed') fail('invalid', `triage callback with status ${payload.status} (on_failed: kundenpipeline.triage.failed)`);
  const { vorgangId, triage } = callback ? await fromCallback(ctx, payload) : direct(payload);
  const vorgang = await loadAnyRecord(ctx, COL_VORGAENGE, vorgangId);
  if (!vorgang) fail('not_found', `unknown Vorgang \`${vorgangId}\``);
  if (callback && (vorgang.status !== 'zugeordnet' || (vorgang.triage_task_id && vorgang.triage_task_id !== payload.task_id))) {
    return { ok: true, vorgang_id: vorgangId, applied: false };
  }
  const now = ctx.now();
  vorgang.triage_json = triage;
  vorgang.status = 'triagiert';
  vorgang.updated_at_ms = now;
  // Rust: Akteur fest "triage-agent", Kanal "system" – unabhaengig vom Sender.
  pushAudit(vorgang, now, 'triage:vorschlag', 'triage-agent', 'system');
  await upsert(ctx, COL_VORGAENGE, vorgang);

  const zeilen = ['▸ MAIL', ...wrapText(cleanBodyOr(vorgang), WRAP_WIDTH)];
  if (typeof triage.antwort_vorschlag === 'string') {
    zeilen.push('', '▸ ANTWORT-VORSCHLAG', ...wrapText(triage.antwort_vorschlag, WRAP_WIDTH));
  }
  if (isObject(triage.aufgabe)) {
    const agent = typeof triage.aufgabe.agent === 'string' ? triage.aufgabe.agent : 'Agent';
    zeilen.push('', `▸ AUFGABE → ${agent}`);
    if (typeof triage.aufgabe.beschreibung === 'string') zeilen.push(...wrapText(triage.aufgabe.beschreibung, WRAP_WIDTH));
  }
  const titel = typeof vorgang.kunde_name === 'string' && vorgang.kunde_name ? vorgang.kunde_name : senderAddressOf(vorgang);
  const decisionId = deterministicId('kpl-e-triage', vorgangId);
  const decision = decisionRecord(decisionId, vorgangId, 'triage', kurz(titel, 40), zeilen, await workspaceDecisionUserIds(ctx), now);
  await upsert(ctx, COL_ENTSCHEIDUNGEN, decision);
  return { ok: true, vorgang_id: vorgangId, decision_id: decisionId };
}

function direct(payload) {
  if (typeof payload.vorgang_id !== 'string') fail('invalid', 'vorgang_id is required');
  if (!isObject(payload.triage)) fail('invalid', 'triage object is required');
  return { vorgangId: payload.vorgang_id, triage: payload.triage };
}
