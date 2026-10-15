// K24-Probe – Bibliothek (Präfix „_“, §16.19): Agentenläufe in der eigenen Collection k24_runs.
export const RUNS = 'k24_runs';
export const SKILL = 'k24-probe/k24-summary';
export const PROMPT = 'prompts/k24-summary.md';

export function fail(code, message) {
  // eslint-disable-next-line no-throw-literal
  throw { code, message };
}

// Prompt-Template aus dem App-Paket (§16.30 Nr. 4). Der Lesezugriff des Handlers auf eigene Paketdateien ist im
// Paketdateien liest der Handler über ctx.asset(pfad) → Text (§16.34). Fehlt es, scheitert der Befehl laut.
export async function readPrompt(ctx) {
  if (typeof ctx.asset !== 'function') fail('unavailable', `Handler-Kontext ohne ctx.asset – ${PROMPT} nicht lesbar`);
  return String(await ctx.asset(PROMPT));
}

export function fill(template, values) {
  return template.replace(/\{\{(\w+)\}\}/g, (all, key) => (key in values ? String(values[key]) : all));
}

// Rückruf (§16.30 Nr. 3): payload {task_id, status, result, workspace_refs}. Der Core liefert höchstens einmal je
// task_id; callback_count macht eine Doppelzustellung trotzdem sichtbar.
export async function settle(command, ctx, outcome) {
  const taskId = command?.payload?.task_id;
  if (typeof taskId !== 'string' || !taskId) fail('invalid', 'Rückruf ohne task_id');
  const [run] = await ctx.read(RUNS, { selector: { task_id: taskId } });
  if (!run) fail('not_found', `kein Lauf zu task_id ${taskId}`);
  const result = command.payload.result;
  const text = typeof result === 'string' ? result : result == null ? '' : JSON.stringify(result);
  await ctx.put(RUNS, {
    ...run,
    status: outcome,
    task_status: String(command.payload.status ?? outcome),
    result_text: text.slice(0, 2000),
    callback_count: (run.callback_count ?? 0) + 1,
    updated_at_ms: Number(ctx.now()),
  });
  return { run_id: run.id };
}
