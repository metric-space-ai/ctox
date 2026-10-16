// Origin: CTOX
// License: Apache-2.0
//
// Seed-Runbooks spreadsheet.summarize / spreadsheet.audit-formulas / spreadsheet.risk-review (Strang SKL-B3, Kern
// §16.30/§16.34). Bis SKL-B3 hatten die Typen keinen Handler: der generische Durchfall store.rs::record_command baute
// aus payload.prompt bzw. „Execute this Business OS automation through CTOX.“ + JSON-Vorschau (command_prompt :27009)
// eine Aufgabe ohne Rückweg in die App (R7). Jetzt: Template prompts/<runbook>.md (Wortlaut aus index.js
// SYSTEMATIC_SPREADSHEET_RUNBOOKS) im Rahmen prompts/spreadsheet-runbook.md, System-Skill spreadsheet (Office-Pack
// des Core), Ergebnis über spreadsheet.runbook.complete/failed an der Tabelle (spreadsheets.runbook_runs[<runbook>]).
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { clampText, fail } from './office.js';

export const SPREADSHEET_SKILL = 'spreadsheet';
export const RUNBOOK_COMPLETE = 'spreadsheet.runbook.complete';
export const RUNBOOK_FAILED = 'spreadsheet.runbook.failed';
export const SPREADSHEET_RUNBOOKS = Object.freeze({
  'spreadsheet.summarize': 'Tabelle zusammenfassen',
  'spreadsheet.audit-formulas': 'Formeln auditieren',
  'spreadsheet.risk-review': 'Finanzielle Risikoanalyse',
});

const RUNS = 'runbook_runs';
const EXCERPT_CHARS = 6000;
const text = (v) => (typeof v === 'string' ? v.trim() : '');
const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const block = (value) => (value ? `${value}\n` : '');
const finish = (rendered) => rendered.replace(/\s+$/, '');

export async function enqueueSpreadsheetRunbook(command, ctx) {
  const runbookId = command.type;
  if (!SPREADSHEET_RUNBOOKS[runbookId]) fail('invalid', `unknown spreadsheet runbook ${runbookId}`);
  const p = isObject(command.payload) ? command.payload : {};
  const sheetId = [command.record_id, p.record_id, p.spreadsheet_id].map(text).find(Boolean);
  if (!sheetId) fail('invalid', `${runbookId} requires spreadsheet_id`);
  const sheet = await ctx.get('spreadsheets', sheetId);
  if (!sheet || sheet.is_deleted) fail('not_found', `spreadsheet ${sheetId} not found`);
  const versionId = text(p.version_id) || text(sheet.current_version_id);
  const version = versionId ? await ctx.get('spreadsheet_versions', versionId) : null;
  const prompt = text(p.prompt);

  const instruction = finish(await renderPrompt(ctx, 'spreadsheet-runbook', {
    runbook_prompt: finish(await renderPrompt(ctx, runbookId.replace(/\./g, '-'), {})),
    title: text(sheet.title) || sheetId,
    filename: text(sheet.filename) || '–',
    spreadsheet_id: sheetId,
    version_id: versionId || '–',
    file_ref: text(version?.source_sha256) || text(sheet.source_sha256) || '–',
    prompt_block: block(prompt ? `Zusatzauftrag: ${prompt}` : ''),
    excerpt: [...String(sheet.index_text ?? '')].slice(0, EXCERPT_CHARS).join('') || '–',
  }));
  const source = { id: command.id, type: runbookId, record_id: sheetId, payload: { spreadsheet_id: sheetId, version_id: versionId, prompt } };
  const task = await ctx.cap.agent.task.enqueue({
    title: `${SPREADSHEET_RUNBOOKS[runbookId]}: ${text(sheet.title) || sheetId}`,
    instruction,
    skill: SPREADSHEET_SKILL,
    thread_key: 'business-os/spreadsheets',
    payload: { source_command: source },
    idempotency_key: `spreadsheets:${command.id}`,
    on_complete: RUNBOOK_COMPLETE,
    on_failed: RUNBOOK_FAILED,
  });
  const taskId = task?.task_id ?? null;
  const now = ctx.now();
  await ctx.transaction(async (tx) => {
    const current = await tx.get('spreadsheets', sheetId);
    if (!current) return;
    const runs = isObject(current[RUNS]) ? current[RUNS] : {};
    await tx.patch('spreadsheets', sheetId, {
      [RUNS]: { ...runs, [runbookId]: { task_id: taskId, command_id: command.id, version_id: versionId || null, status: 'queued', updated_at_ms: now } },
    });
  });
  return { ok: true, status: 'queued', task_id: taskId, spreadsheet_id: sheetId, runbook_id: runbookId };
}

function resultText(result) {
  if (typeof result === 'string') return clampText(result);
  if (isObject(result) && typeof result.text === 'string') return clampText(result.text);
  return clampText(result == null ? '' : JSON.stringify(result));
}

function callbackError(payload) {
  const status = text(payload?.status) === 'cancelled' ? 'cancelled' : 'failed';
  const raw = payload?.error;
  const message = typeof raw === 'string' ? raw : text(raw?.message) || text(payload?.status_note) || status;
  return { status, message: message.replace(/\s+/g, ' ').trim().slice(0, 1000) };
}

// Rückruf: nur der laufende Eintrag (gleiche task_id, Status queued) wird abgeschlossen. Doppelte oder von einem
// neueren Lauf überholte Zustellungen bleiben ohne Wirkung.
export async function settleSpreadsheetRunbook(command, ctx, outcome) {
  const taskId = text(command.payload?.task_id);
  if (!taskId) fail('invalid', `${command.type} is missing task_id`);
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  const source = task?.payload?.source_command;
  if (!isObject(source) || !SPREADSHEET_RUNBOOKS[source.type] || !text(source.record_id)) {
    fail('not_found', `task ${taskId} carries no spreadsheets runbook command`);
  }
  const failure = outcome === 'completed' ? null : callbackError(command.payload);
  const settled = failure
    ? { status: failure.status, result_text: null, error: failure.message }
    : { status: 'completed', result_text: resultText(command.payload?.result), error: null };
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const sheet = await tx.get('spreadsheets', source.record_id);
    const run = isObject(sheet?.[RUNS]) ? sheet[RUNS][source.type] : null;
    if (!run || run.task_id !== taskId || run.status !== 'queued') {
      return { ok: true, status: 'ignored', reason: !sheet ? 'spreadsheet_missing' : 'not_pending', task_id: taskId, spreadsheet_id: source.record_id };
    }
    await tx.patch('spreadsheets', source.record_id, { [RUNS]: { ...sheet[RUNS], [source.type]: { ...run, ...settled, updated_at_ms: now } } });
    return { ok: outcome === 'completed', status: settled.status, task_id: taskId, spreadsheet_id: source.record_id, runbook_id: source.type };
  });
}
