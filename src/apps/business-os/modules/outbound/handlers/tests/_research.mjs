// Test-Helfer Strang OUT3: Recherche über agent.task.enqueue + Rückruf <type>.done (statt research.person).
import { createTestCtx, cmd } from './_ctx.mjs';

/** ctx mit knowledge.query-Protokoll und Agenten-Attrappe (enqueue merkt die Aufgabe, get liefert sie zurück). */
export function researchCtx({ now = 1_000, log = [] } = {}) {
  const tasks = new Map();
  const t = createTestCtx({ now, caps: {
    'knowledge.query': async ({ argv }) => { log.push(argv); return { ok: true, rows: [] }; },
    'agent.task.enqueue': async (args) => { const id = `task_${tasks.size + 1}`; tasks.set(id, JSON.parse(JSON.stringify(args))); return { task_id: id }; },
    'agent.task.get': async ({ task_id }) => (tasks.has(task_id) ? { task_id, status: 'completed', payload: tasks.get(task_id).payload } : null),
  } });
  return { t, log, tasks };
}

/** Phase 1 + Rückruf mit `result` (Recherche-JSON der Rust-Engine) → Ergebnis des Rückrufs. */
export async function runResearch(handle, done, t, command, result, status = 'completed') {
  const accepted = await handle(command, t.ctx);
  const callback = cmd(`${command.type}.done`, { task_id: accepted.task_id, status, result }, { id: `${command.id}:done` });
  return { accepted, outcome: await done(callback, t.ctx) };
}
