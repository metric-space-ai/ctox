// outbound.pipeline.contact_research.done — Rückruf des Kerns (§16.30 Nr. 3, on_complete/on_failed) nach der
// Recherche-Aufgabe aus outbound.pipeline.contact_research (Strang OUT3). payload {task_id, status, result}.
// completed → Phase 2 (applyContactResearch; „keine öffentlichen Kontakte“ bleibt not_found wie Rust :530);
// failed/blocked/cancelled → Laufstatus `failed` in der Runs-Tabelle, keine Kontaktzeilen.
import { loadResearchCallback, parseResearchResult, failureText } from './_lib/research-task.js';
import { applyContactResearch, contactResearchInput } from './outbound.pipeline.contact_research.js';
import { appendRunStatus } from './_lib/knowledge.js';

export default async function handle(command, ctx) {
  const { taskId, status, original, result } = await loadResearchCallback(command, ctx, 'outbound.pipeline.contact_research');
  if (status !== 'completed') {
    const { refs } = contactResearchInput(original);
    await appendRunStatus(ctx, refs, original, 'contact_research', 'failed', ctx.now());
    return { ok: false, task_id: taskId, status, error: failureText(command) };
  }
  return { ...(await applyContactResearch(ctx, original, parseResearchResult(result))), task_id: taskId };
}
