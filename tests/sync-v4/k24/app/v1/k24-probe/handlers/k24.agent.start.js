// K24-Probe – Knopf „Agent“: api.command('k24.agent.start') → Template aus prompts/ füllen → ctx.cap.agent.task.enqueue
// mit dem mitgebrachten Skill und Rückruf in die eigene App (§16.30 Nr. 2–4).
import { RUNS, SKILL, fail, fill, readPrompt } from './_runs.js';

export default async function handle(command, ctx) {
  const label = typeof command?.payload?.label === 'string' ? command.payload.label.trim() : '';
  if (!label) fail('invalid', 'k24.agent.start: label fehlt');
  const runId = `run:${command.id}`;
  const instruction = fill(await readPrompt(ctx), { label, run_id: runId });
  const { task_id: taskId } = await ctx.cap.agent.task.enqueue({
    title: `K24 Probe: ${label.slice(0, 80)}`,
    instruction,
    skill: SKILL,
    payload: { run_id: runId, label },
    idempotency_key: runId,
    on_complete: 'k24.agent.done',
    on_failed: 'k24.agent.failed',
  });
  const now = Number(ctx.now());
  await ctx.put(RUNS, { id: runId, label, status: 'queued', task_id: String(taskId), callback_count: 0, created_at_ms: now, updated_at_ms: now });
  return { run_id: runId, task_id: taskId };
}
