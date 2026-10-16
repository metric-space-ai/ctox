// kundenpipeline.decision.answer – Port aus decision_hub.rs handle_decision_answer :524
// und run_acceptance_followup :630.
// Antwort des Owners (Desktop, Brille, MCP – derselbe Befehl, dieselbe Wirkung):
// - agent_escalation -> resolve_agent_decision (wert = option_id)
// - sonst wert annehmen|ablehnen -> Status entschieden|abgelehnt, Audit am Vorgang
// - Folge einer Annahme genau hier: mailfreigabe -> Mail, triage -> Delegation.
//   Ein Fehler der Folge rollt die Antwort NICHT zurueck, er steht in folge.error.

import {
  AGENT_DECISION_TYPE,
  COL_ENTSCHEIDUNGEN,
  COL_VORGAENGE,
  actorId,
  fail,
  loadAnyRecord,
  pushAudit,
  resolveAgentDecision,
  upsert,
} from './_lib/hub.js';
import { sendVorgangMail } from './kundenpipeline.mail.send.js';
import { delegateVorgang } from './kundenpipeline.delegate.js';

const str = (v) => (typeof v === 'string' ? v : undefined);

// Rust formatiert Fehler mit `{error:#}` (Kette). Hier: Nachricht des geworfenen Objekts.
const errorText = (error) => (error && typeof error === 'object' && 'message' in error ? String(error.message) : String(error));

// ref: decision_hub.rs run_acceptance_followup :630
export async function runAcceptanceFollowup(ctx, command, typ, vorgangId) {
  // Rust: das Laden des Vorgangs propagiert Fehler (`?`), nur die Folge selbst wird
  // als folge.error berichtet.
  let run;
  if (typ === 'mailfreigabe') {
    const v = await loadAnyRecord(ctx, COL_VORGAENGE, vorgangId);
    const art = v && v.status === 'ergebnisVorliegend' ? 'ergebnis' : 'bestaetigung';
    run = () => sendVorgangMail(ctx, command, { vorgang_id: vorgangId, art });
  } else if (typ === 'triage') {
    const v = await loadAnyRecord(ctx, COL_VORGAENGE, vorgangId);
    const beschreibung = str(v?.triage_json?.aufgabe?.beschreibung);
    if (beschreibung === undefined || !beschreibung.trim()) return { skipped: 'triage without a task description' };
    run = () => delegateVorgang(ctx, command, { vorgang_id: vorgangId, beschreibung });
  } else {
    return null;
  }
  try {
    return await run();
  } catch (error) {
    return { error: errorText(error) };
  }
}

export default async function handle(command, ctx) {
  const payload = command.payload ?? {};
  if (typeof payload.entscheidung_id !== 'string') fail('invalid', 'entscheidung_id is required');
  const decisionId = payload.entscheidung_id;
  const wert = (str(payload.wert) ?? '').trim();
  const kanal = str(payload.kanal) ?? 'desktop';
  const decision = await loadAnyRecord(ctx, COL_ENTSCHEIDUNGEN, decisionId);
  if (!decision) fail('not_found', `unknown Entscheidung \`${decisionId}\``);
  if (decision.typ === AGENT_DECISION_TYPE) {
    return resolveAgentDecision(
      ctx,
      { entscheidung_id: decisionId, option_id: wert, comment: payload.comment ?? null, kanal },
      command,
    );
  }
  let status;
  if (wert === 'annehmen') status = 'entschieden';
  else if (wert === 'ablehnen') status = 'abgelehnt';
  else fail('invalid', `unknown decision answer \`${wert}\``);
  const now = ctx.now();
  const akteur = actorId(command, 'owner');
  decision.status = status;
  decision.updated_at_ms = now;
  if (decision.antwort_json === null || typeof decision.antwort_json !== 'object' || Array.isArray(decision.antwort_json)) {
    decision.antwort_json = {};
  }
  decision.antwort_json.wert = wert;
  decision.antwort_json.kanal = kanal;
  decision.antwort_json.zeit_ms = now;
  decision.antwort_json.akteur = akteur;
  await upsert(ctx, COL_ENTSCHEIDUNGEN, decision);

  const vorgangId = str(payload.vorgang_id) ?? str(decision.vorgang_id) ?? '';
  if (vorgangId) {
    const vorgang = await loadAnyRecord(ctx, COL_VORGAENGE, vorgangId);
    if (vorgang) {
      const typ = str(decision.typ) ?? '';
      pushAudit(vorgang, now, `${typ}:${wert}`, akteur, kanal);
      vorgang.updated_at_ms = now;
      await upsert(ctx, COL_VORGAENGE, vorgang);
    }
  }
  let folge = null;
  if (status === 'entschieden' && vorgangId) {
    folge = await runAcceptanceFollowup(ctx, command, str(decision.typ) ?? '', vorgangId);
  }
  return { ok: true, entscheidung_id: decisionId, status, folge };
}
