// Origin: CTOX
// License: Apache-2.0
// research.systematic.report.create – Knopf „Neues Dokument“ / „Runbook starten“ / Import mit Runbook (Strang
// SKL-B3, Kern §16.30). Füllt prompts/report.md (Wortlaut bis SKL-B3: index.js dispatchNewDocumentReport) mit dem
// Runbook-Template prompts/research-report-*.md (bis SKL-B3: SYSTEMATIC_REPORT_RUNBOOKS.prompt_template) und legt
// eine Agentenaufgabe mit dem System-Skill systematic-research an. Ergebnis kommt als Rückruf
// research.systematic.report.complete (DOCX-Rückschrieb) bzw. research.systematic.report.failed.
// Ersetzt: store.rs record_command + command_prompt :27009 + suggested_skill_for_command :27736 („research.systematic.*“).
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import { expectedDocxFilename, fail } from './_lib/report.js';
import {
  REPORT_COMPLETE, REPORT_FAILED, REPORT_RUNBOOKS, REPORT_SKILL,
  finish, isObject, knowledgeInstruction, promptName, slugFilename, text,
} from './_lib/automation.js';

const OUTPUT_DIR = 'runtime/business-os/documents/generated';
// Nur noch Zustand, kein Prompt-Text: Felder, die der Agent nicht braucht oder die der Handler selbst setzt.
const DROPPED = ['instruction', 'required_skills', 'selected_runbook', 'prompt_template', 'client_context', 'module'];

export default async function handle(command, ctx) {
  if (!text(command.id)) fail('invalid', 'documents report command id is required');
  const p = isObject(command.payload) ? command.payload : {};
  const recordId = [command.record_id, p.record_id, p.document_id, p.writeback_contract?.document_id].map(text).find(Boolean) || '';
  const existing = recordId ? await ctx.get('documents', recordId) : null;
  const runbookId = [p.selected_runbook_id, p.runbook_id].map(text).find(Boolean) || 'research.report.auto';
  if (!REPORT_RUNBOOKS[runbookId]) fail('invalid', `unknown report runbook ${runbookId}`);
  const reportType = text(p.report_type_id) || runbookId.replace(/^research\.report\./, '') || 'auto';
  const title = text(p.title) || text(existing?.title) || 'Research Document';
  const filename = expectedDocxFilename(command) ?? `${slugFilename(title)}.docx`;
  const outputPath = text(p.output_path) || `${OUTPUT_DIR}/${filename}`;
  const knowledge = isObject(p.knowledge_context) ? p.knowledge_context : null;

  const runbookPrompt = finish(await renderPrompt(ctx, promptName(runbookId), {}));
  const reportTypeLine = finish(await renderPrompt(ctx, reportType === 'auto' ? 'report-type-auto' : 'report-type-fixed', { report_type_id: reportType }));
  const instruction = finish(await renderPrompt(ctx, 'report', {
    title,
    prompt: text(p.prompt) || runbookPrompt,
    knowledge_context: await knowledgeInstruction(ctx, knowledge),
    report_type: reportTypeLine,
    output_path: outputPath,
    runbook_title: REPORT_RUNBOOKS[runbookId],
    runbook_prompt: runbookPrompt,
  }));

  const sourcePayload = Object.fromEntries(Object.entries(p).filter(([key]) => !DROPPED.includes(key)));
  const source = {
    id: command.id,
    type: command.type,
    record_id: recordId,
    payload: {
      ...sourcePayload,
      title, report_type_id: reportType, selected_runbook_id: runbookId,
      output_filename: filename, output_path: outputPath, desired_format: 'docx',
    },
    client_context: isObject(command.client_context) ? command.client_context : (isObject(p.client_context) ? p.client_context : {}),
  };
  const task = await ctx.cap.agent.task.enqueue({
    title: `Word-Dokument: ${title}`,
    instruction,
    skill: REPORT_SKILL,
    thread_key: 'business-os/documents',
    payload: { source_command: source },
    idempotency_key: `documents:${command.id}`,
    on_complete: REPORT_COMPLETE,
    on_failed: REPORT_FAILED,
  });
  return { ok: true, status: 'queued', task_id: task?.task_id ?? null, document_id: recordId || null, filename, output_path: outputPath };
}
