// Origin: CTOX
// License: Apache-2.0
//
// Seed-Runbooks document.summarize / document.extract-requirements / document.review-risks (Strang SKL-B3, Kern
// §16.30). Bis SKL-B3 hatten die Typen keinen Handler: der generische Durchfall store.rs::record_command baute aus
// payload.prompt bzw. „Execute this Business OS automation through CTOX.“ + JSON-Vorschau (command_prompt :27009)
// eine Aufgabe ohne Rückweg in die App (R7, nur Chat-Projektion). Jetzt: Template prompts/<runbook>.md
// (Wortlaut der Seed-Runbooks, index.js ensureSeedRunbooks) im Rahmen prompts/document-runbook.md, System-Skill doc,
// Ergebnis über document.runbook.complete/failed am Dokument (documents.runbook_runs[<runbook>]).
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { fail } from './report.js';
import {
  DOCUMENT_RUNBOOKS, DOCUMENT_SKILL, EXCERPT_CHARS, RUNBOOK_COMPLETE, RUNBOOK_FAILED,
  block, callbackError, finish, isObject, knowledgeInstruction, promptName, resultText, sourceOfCallback, text,
} from './automation.js';

const RUNS = 'runbook_runs';

export async function enqueueDocumentRunbook(command, ctx) {
  const runbookId = command.type;
  if (!DOCUMENT_RUNBOOKS[runbookId]) fail('invalid', `unknown document runbook ${runbookId}`);
  const p = isObject(command.payload) ? command.payload : {};
  const documentId = [command.record_id, p.record_id, p.document_id].map(text).find(Boolean);
  if (!documentId) fail('invalid', `${runbookId} requires document_id`);
  const doc = await ctx.get('documents', documentId);
  if (!doc || doc.is_deleted) fail('not_found', `document ${documentId} not found`);
  const versionId = text(p.version_id) || text(doc.current_version_id);
  const version = versionId ? await ctx.get('document_versions', versionId) : null;
  const prompt = text(p.prompt);
  const knowledge = isObject(p.knowledge_context) && text(p.knowledge_context.id) ? p.knowledge_context : null;

  const instruction = finish(await renderPrompt(ctx, 'document-runbook', {
    runbook_prompt: finish(await renderPrompt(ctx, promptName(runbookId), {})),
    title: text(doc.title) || documentId,
    filename: text(doc.filename) || '–',
    document_id: documentId,
    version_id: versionId || '–',
    file_ref: text(version?.source_sha256) || text(doc.source_sha256) || '–',
    prompt_block: block(prompt ? `Zusatzauftrag: ${prompt}` : ''),
    knowledge_context: knowledge ? await knowledgeInstruction(ctx, knowledge) : '',
    excerpt: [...String(doc.index_text ?? '')].slice(0, EXCERPT_CHARS).join('') || '–',
  }));
  const source = { id: command.id, type: runbookId, record_id: documentId, payload: { document_id: documentId, version_id: versionId, prompt } };
  const task = await ctx.cap.agent.task.enqueue({
    title: `${DOCUMENT_RUNBOOKS[runbookId]}: ${text(doc.title) || documentId}`,
    instruction,
    skill: DOCUMENT_SKILL,
    thread_key: 'business-os/documents',
    payload: { source_command: source },
    idempotency_key: `documents:${command.id}`,
    on_complete: RUNBOOK_COMPLETE,
    on_failed: RUNBOOK_FAILED,
  });
  const taskId = task?.task_id ?? null;
  const now = ctx.now();
  await ctx.transaction(async (tx) => {
    const current = await tx.get('documents', documentId);
    if (!current) return;
    const runs = isObject(current[RUNS]) ? current[RUNS] : {};
    await tx.patch('documents', documentId, {
      [RUNS]: { ...runs, [runbookId]: { task_id: taskId, command_id: command.id, version_id: versionId || null, status: 'queued', updated_at_ms: now } },
    });
  });
  return { ok: true, status: 'queued', task_id: taskId, document_id: documentId, runbook_id: runbookId };
}

// Rückruf: nur der laufende Eintrag (gleiche task_id, Status queued) wird abgeschlossen. Doppelte oder von einem
// neueren Lauf überholte Zustellungen bleiben ohne Wirkung.
export async function settleDocumentRunbook(command, ctx, outcome) {
  const { taskId, source } = await sourceOfCallback(command, ctx);
  const documentId = source.record_id;
  const runbookId = source.type;
  const now = ctx.now();
  const failure = outcome === 'completed' ? null : callbackError(command.payload);
  const settled = failure
    ? { status: failure.status, result_text: null, error: failure.message }
    : { status: 'completed', result_text: resultText(command.payload?.result), error: null };
  return ctx.transaction(async (tx) => {
    const doc = await tx.get('documents', documentId);
    const run = isObject(doc?.[RUNS]) ? doc[RUNS][runbookId] : null;
    if (!run || run.task_id !== taskId || run.status !== 'queued') {
      return { ok: true, status: 'ignored', reason: !doc ? 'document_missing' : 'not_pending', task_id: taskId, document_id: documentId };
    }
    await tx.patch('documents', documentId, { [RUNS]: { ...doc[RUNS], [runbookId]: { ...run, ...settled, updated_at_ms: now } } });
    return { ok: outcome === 'completed', status: settled.status, task_id: taskId, document_id: documentId, runbook_id: runbookId };
  });
}
