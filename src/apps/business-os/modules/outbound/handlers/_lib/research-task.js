// Recherche als Agentenauftrag (Strang OUT3, ersetzt die Core-Fähigkeit research.person).
//
// Vorher: Core-Fähigkeit research.person({company, country, mode, fields}) = ctox_web_stack::run_ctox_person_research_tool
// (importer.rs:3266) – eine komplette Recherche-Engine im Core (Suchplan, Quellenwahl, Feldextraktion), also
// App-Logik hinter einer Fach-Capability. Jetzt: der Handler stellt einen Auftrag über das generische Primitiv
// agent.task.enqueue (§16.30, flach) mit on_complete/on_failed = eigener Rückruf-Befehl `<type>.done`; der Rückruf
// liest den Ausgangsbefehl aus der Aufgabe (agent.task.get → payload.command) und führt die unveränderte
// Nachverarbeitung aus (Feldabbildung, Fit-Score, Kontakte, DSGVO-Stempel, Knowledge-Zeilen).
// Das Ergebnisformat ist das der Rust-Engine ({fields:{<FieldKey>:{value, source_url?, candidates?[]}}}), damit
// _lib/research.js (applyCompanyResearchFields, evidenceFromResearch, contactsFromResearch) unverändert gilt.
import { fail, isObject } from './outbound.js';

export const TERMINAL_FAILED = ['failed', 'blocked', 'cancelled'];

const MODE_TEXT = {
  new_record: 'Firmendaten für einen neuen Datensatz',
  update_person: 'Ansprechpartner (Personen) der Firma',
};

/** Anweisung an den Agenten; Felder sind FieldKey::as_str-Namen (src/tools/web-stack/src/sources/mod.rs:209). */
export function researchInstruction(request) {
  return [
    `Recherchiere ${MODE_TEXT[request.mode] ?? request.mode} aus öffentlichen Quellen (Rechercheauftrag der App outbound).`,
    `- Firma: ${request.company}`,
    `- Land: ${request.country}`,
    `- Modus: ${request.mode}`,
    `- Felder: ${request.fields.join(', ')}`,
    'Nutze die CTOX-Websuche/Browser-Recherche. Belege jeden Wert mit der Quelle (source_url, bei Personen je Kandidat).',
    'Erfinde keine Werte; Felder ohne Beleg lässt du weg. Bei Personen: jeder Kandidat aus genau einer Quelle.',
    'Antworte ausschließlich mit einem JSON-Objekt dieser Form:',
    '{"fields":{"<feld>":{"value":"…","source_url":"…","candidates":[{"value":"…","source_url":"…","source_id":"…","confidence":0.0}]}}}',
  ].join('\n');
}

/** Ausgangsbefehl, den der Rückruf braucht (Identität, Payload, Kontext). */
const originalOf = (command) => ({
  id: command.id,
  type: command.type,
  app_id: command.app_id,
  ...(command.record_id !== undefined ? { record_id: command.record_id } : {}),
  ...(isObject(command.client_context) ? { client_context: command.client_context } : {}),
  payload: isObject(command.payload) ? command.payload : {},
});

/** App-Skill der Recherche (Strang SKL-O: Paketkopie von src/skills/system/research/prospect-research). */
export const RESEARCH_SKILL = 'outbound/prospect-research';

export async function enqueueResearch(ctx, command, { title, request }) {
  const done = `${command.type}.done`;
  const task = await ctx.cap.agent.task.enqueue({
    title,
    instruction: researchInstruction(request),
    skill: RESEARCH_SKILL,
    thread_key: `business-os/outbound/research/${command.id}`,
    idempotency_key: `${command.type}:${command.id}`,
    payload: { command: originalOf(command), research_request: request },
    on_complete: done,
    on_failed: done,
  });
  const taskId = typeof task?.task_id === 'string' ? task.task_id.trim() : '';
  if (taskId === '') fail('unavailable', 'agent task enqueue did not return task_id');
  return { ok: true, status: 'accepted', execution_mode: 'agent_task', task_id: taskId, callback: done, research_request: request };
}

/** Rückruf §16.30 Nr. 3 {task_id, status, result}: Ausgangsbefehl aus der Aufgabe, nur vom erwarteten Typ. */
export async function loadResearchCallback(command, ctx, expectedType) {
  const payload = isObject(command.payload) ? command.payload : {};
  const taskId = typeof payload.task_id === 'string' ? payload.task_id.trim() : '';
  if (taskId === '') fail('invalid', `${expectedType}.done is missing task_id`);
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  const original = task?.payload?.command;
  if (!isObject(original) || original.type !== expectedType) fail('not_found', `task ${taskId} carries no ${expectedType} command`);
  const status = typeof payload.status === 'string' ? payload.status : '';
  if (status !== 'completed' && !TERMINAL_FAILED.includes(status)) fail('invalid', `${expectedType}.done: unknown task status ${status}`);
  return { taskId, status, original, result: payload.result };
}

/** Recherche-JSON aus dem Agentenergebnis (Objekt, {text}, oder Text mit eingebettetem JSON-Objekt). */
export function parseResearchResult(result) {
  if (isObject(result) && isObject(result.fields)) return result;
  const text = typeof result === 'string' ? result : isObject(result) && typeof result.text === 'string' ? result.text : null;
  if (text != null) {
    const start = text.indexOf('{');
    const end = text.lastIndexOf('}');
    if (start >= 0 && end > start) {
      try {
        const parsed = JSON.parse(text.slice(start, end + 1));
        if (isObject(parsed) && isObject(parsed.fields)) return parsed;
      } catch { /* fällt unten durch */ }
    }
  }
  fail('invalid', 'research task returned no research JSON with fields');
}

export function failureText(command) {
  const p = isObject(command.payload) ? command.payload : {};
  const note = [p.status_note, p.result?.status_note, p.result?.error, p.result?.message].find((v) => typeof v === 'string' && v.trim() !== '');
  return note ? note.trim() : `agent task ${p.status}`;
}
