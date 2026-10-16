// Origin: CTOX
// License: Apache-2.0
// research.systematic.report.complete – on_complete-Rückruf (Kern §16.30) der Aufgabe aus
// research.systematic.report.create, kein UI-Sender. payload {task_id, status, result, workspace_refs?}.
// Auswertung wie bisher nativ im Core: store.rs:10325 complete_business_command_from_queue_reply →
// :21471 process_documents_report_command (Lineage + DOCX-Rückschrieb, _lib/report-writeback.js); die Antwort des
// Agenten (JSON mit knowledge_version_id, source_receipt_links, optional file_ref) ergänzt den Ausgangsbefehl wie
// früher reply_text (:24800 docx_paths_from_text). Idempotent: je Ausgangsbefehl höchstens eine Version.
import { writebackGeneratedDocx } from './_lib/report-writeback.js';
import { resultObject, sourceOfCallback, text } from './_lib/automation.js';

// Felder, die die Antwort beisteuern darf; Dateiname, Zielpfad und Dokument-ID bestimmt der Ausgangsbefehl.
const ANSWER_KEYS = ['knowledge_version', 'knowledge_version_id', 'source_receipt_links', 'source_receipts', 'output_file_ref', 'file_ref', 'blob_ref'];

export default async function handle(command, ctx) {
  const { taskId, source } = await sourceOfCallback(command, ctx);
  const [done] = await ctx.read('document_versions', { selector: { business_command_id: source.id }, limit: 1 });
  if (done) return { ok: true, status: 'ignored', reason: 'duplicate', task_id: taskId, document_id: done.document_id, version_id: done.id };

  const answer = resultObject(command.payload?.result);
  const picked = Object.fromEntries(ANSWER_KEYS.filter((key) => answer[key] !== undefined && answer[key] !== null).map((key) => [key, answer[key]]));
  const merged = { ...source, payload: { ...source.payload, ...picked } };
  const refs = Array.isArray(command.payload?.workspace_refs) ? command.payload.workspace_refs : [];
  const outputPath = text(source.payload?.output_path);
  const workspaceRef = async (filename) => {
    const path = refs.find((ref) => ref === outputPath || ref === filename || String(ref).endsWith(`/${filename}`));
    if (!path) return undefined;
    const file = await ctx.cap.agent.task.workspace.read({ task_id: taskId, path });
    return typeof file?.ref === 'string' ? file.ref : (typeof file?.ref?.ref === 'string' ? file.ref.ref : undefined);
  };
  const result = await writebackGeneratedDocx(merged, ctx, { workspaceRef });
  return { ...result, task_id: taskId };
}
