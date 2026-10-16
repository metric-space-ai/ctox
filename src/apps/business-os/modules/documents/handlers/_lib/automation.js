// Origin: CTOX
// License: Apache-2.0
//
// Automatisierung der documents-App (Kern §16.30/§16.34, Strang SKL-B3): Knopf → api.command → Handler füllt ein
// Template aus prompts/ → ctx.cap.agent.task.enqueue({…, on_complete, on_failed}) → Rückruf als eigener Befehl.
// Ersetzt den generischen Durchfall store.rs::record_command + command_prompt (:27009) +
// suggested_skill_for_command (:27736) für research.systematic.report.create und document.*; die Anweisungstexte
// standen bis SKL-B3 als Literale in index.js (SYSTEMATIC_REPORT_RUNBOOKS, dispatchNewDocumentReport,
// knowledgeContextInstruction, ensureSeedRunbooks) und liegen jetzt wörtlich in prompts/*.md.
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { fail } from './report.js';
import { clampText } from './office.js';

// Core-System-Skills (src/skills/**): generische Recherche-Methode und Office-Pack, keine documents-Logik.
export const REPORT_SKILL = 'systematic-research';
export const DOCUMENT_SKILL = 'doc';

export const REPORT_COMMAND = 'research.systematic.report.create';
export const REPORT_COMPLETE = 'research.systematic.report.complete';
export const REPORT_FAILED = 'research.systematic.report.failed';
export const RUNBOOK_COMPLETE = 'document.runbook.complete';
export const RUNBOOK_FAILED = 'document.runbook.failed';

// Runbooks mit Template im Paket (Name = id mit '-' statt '.'/'_'). Andere ids werden abgelehnt.
export const REPORT_RUNBOOKS = Object.freeze({
  'research.report.auto': 'Deep Research Word-Bericht',
  'research.report.feasibility_study': 'Machbarkeitsstudie',
  'research.report.market_research': 'Marktanalyse',
  'research.report.competitive_analysis': 'Wettbewerbsanalyse',
  'research.report.technology_screening': 'Technologie-Screening',
  'research.report.whitepaper': 'Whitepaper',
  'research.report.literature_review': 'Stand der Technik',
  'research.report.decision_brief': 'Entscheidungsvorlage',
  'research.report.project_description': 'Projektbeschreibung / Fördervorhaben',
  'research.report.source_review': 'Quellenreview',
});
export const DOCUMENT_RUNBOOKS = Object.freeze({
  'document.summarize': 'Zusammenfassen',
  'document.extract-requirements': 'Requirements extrahieren',
  'document.review-risks': 'Risiken prüfen',
});

const EXCERPT_CHARS = 6000;

export const text = (v) => (typeof v === 'string' ? v.trim() : '');
export const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const block = (value) => (value ? `${value}\n` : '');
const finish = (rendered) => rendered.replace(/\s+$/, '');

export function promptName(runbookId) {
  return runbookId.replace(/[._]/g, '-');
}

// ref: index.js knowledgeContextInstruction (bis SKL-B3) – Texte jetzt in prompts/knowledge-*.md.
export async function knowledgeInstruction(ctx, context) {
  if (!isObject(context) || !text(context.id)) return finish(await renderPrompt(ctx, 'knowledge-auto', {}));
  const values = { kind: text(context.kind) || 'skill', title: text(context.title) || text(context.id), id: text(context.id), domain: text(context.domain) || 'ohne Domain' };
  return finish(await renderPrompt(ctx, context.is_procedural_skill ? 'knowledge-skill' : 'knowledge-table', values));
}

// ref: index.js slugFilename/ensureExtension (Dateiname der erzeugten DOCX).
export function slugFilename(value) {
  const base = String(value || '').replace(/\s+/g, ' ').trim().toLowerCase().normalize('NFKD')
    .replace(/[̀-ͯ]/g, '').replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  return base || 'document';
}

/** Ausgangsbefehl aus der Aufgabe des Rückrufs (payload.task_id → agent.task.get → payload.source_command). */
export async function sourceOfCallback(command, ctx) {
  const taskId = text(command.payload?.task_id);
  if (!taskId) fail('invalid', `${command.type} is missing task_id`);
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  const source = task?.payload?.source_command;
  if (!isObject(source) || !text(source.id) || !text(source.type)) fail('not_found', `task ${taskId} carries no documents source command`);
  return { taskId, task, source };
}

/** Fehlertext eines on_failed-Rückrufs (status cancelled = Abbruch). */
export function callbackError(payload) {
  const status = text(payload?.status) || 'failed';
  const raw = payload?.error;
  const message = typeof raw === 'string' ? raw : text(raw?.message) || text(payload?.status_note) || (status === 'cancelled' ? 'cancelled' : 'failed');
  return { status: status === 'cancelled' ? 'cancelled' : 'failed', message: message.replace(/\s+/g, ' ').trim().slice(0, 1000) };
}

/** Ergebnistext eines on_complete-Rückrufs. */
export function resultText(result) {
  if (typeof result === 'string') return clampText(result);
  if (isObject(result) && typeof result.text === 'string') return clampText(result.text);
  return clampText(result == null ? '' : JSON.stringify(result));
}

/** Antwort des Agenten als Objekt: result selbst oder das JSON-Objekt am Ende des Antworttexts. */
export function resultObject(result) {
  if (isObject(result) && typeof result.text !== 'string') return result;
  const raw = (typeof result === 'string' ? result : isObject(result) ? result.text : '').trim();
  const end = raw.lastIndexOf('}');
  for (let i = raw.indexOf('{'); i >= 0 && i < end; i = raw.indexOf('{', i + 1)) {
    try {
      const parsed = JSON.parse(raw.slice(i, end + 1));
      if (isObject(parsed)) return parsed;
    } catch { /* nächste Klammer */ }
  }
  return {};
}

export { EXCERPT_CHARS, block, finish };
