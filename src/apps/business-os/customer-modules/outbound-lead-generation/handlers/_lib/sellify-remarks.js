// Sellify-Vermerkprüfung als App-Handler (Strang SKL-C; Bestandsaufnahme F §1 #27).
//
// Vorher: der Browser schickte `business_os.chat.task` (Prompt freitextPruefungPrompt), pollte den Befehlsstatus und
// parste die Antwort selbst (index.js verarbeiteFreitextAntworten / leseFreitextAntwort / baueFreitextUrteil).
// Jetzt:
//   outbound.sellify.remarks.check     Template prompts/sellify-remarks.md → agent.task.enqueue (kein Skill), Prüfvermerk am Lead
//   outbound.sellify.remarks.complete  on_complete: JSON-Block lesen, Urteil je Lead in den Lead schreiben (live bei allen Clients)
//   outbound.sellify.remarks.failed    on_failed (auch Abbruch): Prüfvermerk mit Fehler, die UI plant nach der Pause neu
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { fail, isObject, str } from './olg.js';
import { LEAD_COLLECTION } from './update-digest.js';
import { failureText, parseJsonBlock, replyText } from './agent-reply.js';
import { jsonPreview } from './lead-research.js';

export const REMARKS_CHECK = 'outbound.sellify.remarks.check';
export const REMARKS_COMPLETE = 'outbound.sellify.remarks.complete';
export const REMARKS_FAILED = 'outbound.sellify.remarks.failed';
// ref: index.js (vor SKL-C :4613-4625) FREITEXT_URTEIL_VERSION, FREITEXT_PRIORITAET, FREITEXT_STATUS
export const FREITEXT_URTEIL_VERSION = 1;
const FREITEXT_PRIORITAET = 'urgent';
const FREITEXT_STATUS = Object.freeze({ gesperrt: 'blocked', pruefen: 'review', frei: 'free' });

const clone = (v) => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)));

// ref: index.js normalizeProtectionText
function normalizeProtectionText(value) {
  return String(value || '').normalize('NFKC').toLocaleLowerCase('de-DE').replace(/[^\p{L}\p{N}]+/gu, ' ').trim().replace(/\s+/g, ' ');
}

// ref: index.js freitextEintrag
function freitextEintrag(roh) {
  const status = String(roh?.status || '').trim().toLowerCase().replace('prüfen', 'pruefen');
  if (!FREITEXT_STATUS[status]) return null;
  return { status, grund: String(roh?.grund || '').trim().slice(0, 300), zitat: String(roh?.zitat || '').trim().slice(0, 600) };
}

/** ref: index.js baueFreitextUrteil — vollständig nur mit Firma und JEDER geprüften Person. */
export function baueFreitextUrteil(roh, pruefung, commandId, nowMs) {
  const firma = freitextEintrag(roh?.firma);
  if (!firma) return null;
  const personen = {};
  for (const personId of pruefung?.pruefpersonen || []) {
    const eintrag = freitextEintrag(roh?.personen?.[personId] ?? roh?.personen?.[String(personId)]);
    if (!eintrag) return null;
    personen[personId] = eintrag;
  }
  const namen = {};
  for (const [name, wert] of Object.entries(roh?.namen || {})) {
    const eintrag = freitextEintrag(wert);
    const schluessel = normalizeProtectionText(name);
    if (eintrag && schluessel.split(' ').length >= 2) namen[schluessel] = eintrag;
  }
  return { version: FREITEXT_URTEIL_VERSION, signatur: pruefung.signatur, command_id: commandId, geprueft_at_ms: nowMs, firma, personen, namen };
}

async function patchLeadPayload(ctx, leadId, patch) {
  const lead = await ctx.get(LEAD_COLLECTION, leadId);
  if (lead == null) return false;
  await ctx.put(LEAD_COLLECTION, { ...lead, payload: { ...(isObject(lead.payload) ? lead.payload : {}), ...patch } });
  return true;
}

/** outbound.sellify.remarks.check {entries:[{lead_id, signatur, pruefpersonen, lead_data}], title?} */
export async function handleRemarksCheck(command, ctx) {
  const p = isObject(command.payload) ? command.payload : {};
  const entries = (Array.isArray(p.entries) ? p.entries : []).filter((e) => str(e, ['lead_id']) && str(e, ['signatur']));
  if (entries.length === 0) fail('invalid', 'entries must not be empty');
  const leadIds = entries.map((e) => e.lead_id.trim());
  const instruction = await renderPrompt(ctx, 'sellify-remarks', {
    payload_json: jsonPreview({ lead_ids: leadIds, freitext_leads: entries.map((e) => clone(e.lead_data ?? { lead_id: e.lead_id })) }),
  });
  const title = str(p, ['title']) ?? (entries.length === 1 ? `Sellify-Vermerk prüfen: ${str(entries[0].lead_data, ['firma']) ?? leadIds[0]}` : `Sellify-Vermerke prüfen: ${entries.length} Leads`);
  const task = await ctx.cap.agent.task.enqueue({
    title,
    instruction,
    thread_key: `business-os/outbound-lead-generation/sperrvermerk/${command.id}`,
    priority: FREITEXT_PRIORITAET,
    idempotency_key: `${REMARKS_CHECK}:${command.id}`,
    payload: { lead_ids: leadIds, entries: entries.map((e) => ({ lead_id: e.lead_id.trim(), signatur: e.signatur, pruefpersonen: clone(e.pruefpersonen ?? []) })) },
    on_complete: REMARKS_COMPLETE,
    on_failed: REMARKS_FAILED,
  });
  const at = ctx.now();
  for (const e of entries) {
    await patchLeadPayload(ctx, e.lead_id.trim(), {
      sellify_freitext_pruefung: { command_id: command.id, task_id: task.task_id, signatur: e.signatur, at_ms: at, pruefpersonen: clone(e.pruefpersonen ?? []) },
    });
  }
  return { ok: true, task_id: task.task_id, lead_ids: leadIds };
}

async function taskEntries(ctx, command, type) {
  const taskId = str(command.payload, ['task_id']);
  if (taskId == null) fail('invalid', `${type} is missing task_id`);
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  if (task == null) fail('not_found', `agent task ${taskId} does not exist`);
  return { taskId, entries: Array.isArray(task.payload?.entries) ? task.payload.entries : [] };
}

// Nur der Prüfvermerk DIESES Tasks zählt (eine neuere Prüfung derselben Lead hat Vorrang).
const sameCheck = (lead, taskId) => lead?.payload?.sellify_freitext_pruefung?.task_id === taskId;

/** outbound.sellify.remarks.complete {task_id, status, result} — Antwort lesen, Urteil je Lead schreiben. */
export async function handleRemarksComplete(command, ctx) {
  if (str(command.payload, ['status']) !== 'completed') return handleRemarksFailed(command, ctx);
  const { taskId, entries } = await taskEntries(ctx, command, REMARKS_COMPLETE);
  const antwort = parseJsonBlock(replyText(command.payload.result), (v) => isObject(v?.leads))?.leads ?? null;
  const out = { ok: true, task_id: taskId, urteile: 0, ohne_urteil: 0, unveraendert: 0 };
  for (const entry of entries) {
    const lead = await ctx.get(LEAD_COLLECTION, entry.lead_id);
    const pruefung = lead?.payload?.sellify_freitext_pruefung;
    if (!sameCheck(lead, taskId) || pruefung.fehler || lead.payload?.sellify_freitext_urteil?.task_id === taskId) { out.unveraendert += 1; continue; }
    const urteil = antwort ? baueFreitextUrteil(antwort[entry.lead_id], pruefung, pruefung.command_id ?? taskId, ctx.now()) : null;
    if (urteil) {
      await patchLeadPayload(ctx, entry.lead_id, { sellify_freitext_urteil: { ...urteil, task_id: taskId } });
      out.urteile += 1;
    } else {
      await patchLeadPayload(ctx, entry.lead_id, { sellify_freitext_pruefung: { ...pruefung, at_ms: ctx.now(), fehler: 'Antwort ohne vollständiges Urteil' } });
      out.ohne_urteil += 1;
    }
  }
  return out;
}

/** outbound.sellify.remarks.failed {task_id, status, error?} */
export async function handleRemarksFailed(command, ctx) {
  const { taskId, entries } = await taskEntries(ctx, command, REMARKS_FAILED);
  const reason = str(command.payload, ['status']) === 'cancelled' ? 'cancelled' : failureText(command.payload);
  let markiert = 0;
  for (const entry of entries) {
    const lead = await ctx.get(LEAD_COLLECTION, entry.lead_id);
    const pruefung = lead?.payload?.sellify_freitext_pruefung;
    if (!sameCheck(lead, taskId) || pruefung.fehler || lead.payload?.sellify_freitext_urteil?.task_id === taskId) continue;
    await patchLeadPayload(ctx, entry.lead_id, { sellify_freitext_pruefung: { ...pruefung, at_ms: ctx.now(), fehler: reason } });
    markiert += 1;
  }
  return { ok: true, task_id: taskId, markiert };
}
