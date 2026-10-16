// outbound.company.research.done — Rückruf des Kerns (§16.30 Nr. 3, on_complete/on_failed) nach der
// Recherche-Aufgabe aus outbound.company.research (Strang OUT3). payload {task_id, status, result, workspace_refs?}.
// completed → Phase 2 (applyCompanyResearch, Rust importer.rs:350-400); failed/blocked/cancelled → Laufstatus
// `failed` in der Runs-Tabelle (Rust: der Befehl scheiterte), keine Firmenzeile.
import { loadResearchCallback, parseResearchResult, failureText } from './_lib/research-task.js';
import { applyCompanyResearch, companyResearchInput } from './outbound.company.research.js';
import { appendRunStatus } from './_lib/knowledge.js';

export default async function handle(command, ctx) {
  const { taskId, status, original, result } = await loadResearchCallback(command, ctx, 'outbound.company.research');
  if (status !== 'completed') {
    const { refs } = companyResearchInput(original);
    await appendRunStatus(ctx, refs, original, 'company_research', 'failed', ctx.now());
    return { ok: false, task_id: taskId, status, error: failureText(command) };
  }
  return { ...(await applyCompanyResearch(ctx, original, parseResearchResult(result))), task_id: taskId };
}
