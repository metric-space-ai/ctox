// Persistenz des Recherche-Abschlusses (store.rs:21570-21799 Erfolg,
// :22820-22943 Fehlschlag). Schreibt research_tasks und research_runs über ctx.transaction;
// business_commands schreibt nur der Befehls-Lebenszyklus (CAP-3b: kein agent.command.settle).
import { clipText, firstStringField } from './util.js';

function displayCommandType(value) {
  return value.replaceAll('business_os.', '').replaceAll('ctox.', '').replace(/[_.-]/g, ' ');
}

// store.rs::command_title
export function commandTitle(command) {
  const title = typeof command.payload?.title === 'string' ? command.payload.title.trim() : '';
  if (title) return title;
  const record = typeof command.record_id === 'string' && command.record_id.trim() ? command.record_id : 'new task';
  return `${displayCommandType(command.command_type)} · ${record}`;
}

export function researchTaskId(command) {
  const rec = typeof command.record_id === 'string' ? command.record_id.trim() : '';
  return rec || firstStringField(command.payload, ['task_id', 'research_task_id']) || '';
}

async function findRunByCommand(tx, commandId) {
  const docs = await tx.read('research_runs', { selector: { command_id: commandId }, limit: 1 });
  return docs[0] ?? null;
}

export async function persistSuccess(ctx, { commandId, command, queueTask, taskQueueId, replyText, evidence, completedAtMs }) {
  const taskId = researchTaskId(command);
  const knowledgeDomain = firstStringField(command.payload, ['knowledge_domain', 'domain']) ??
    firstStringField(command.client_context, ['knowledge_domain', 'domain']) ?? '';
  const result = {
    summary: clipText(replyText, 1200),
    knowledge_domain: knowledgeDomain,
    task_id: taskId,
    task_queue_id: taskQueueId,
    identified_count: evidence.identified_count,
    accepted_count: evidence.accepted_count,
    used_count: evidence.used_count,
    source_receipt_links: evidence.source_receipt_links,
    source_receipt_snapshot_hashes: evidence.source_receipt_snapshot_hashes,
    knowledge_version: evidence.knowledge_version,
    completed_at_ms: completedAtMs,
  };
  await ctx.transaction(async (tx) => {
    if (taskId) {
      const existing = await tx.get('research_tasks', taskId);
      const task = existing ?? {
        id: taskId,
        title: firstStringField(command.payload, ['task_title', 'title']) ?? commandTitle(command),
        status: 'ready',
        knowledge_domain: knowledgeDomain,
        payload: {},
        created_at_ms: completedAtMs,
        updated_at_ms: completedAtMs,
      };
      task.status = 'ready';
      task.updated_at_ms = completedAtMs;
      if (!Object.hasOwn(task, 'knowledge_domain')) task.knowledge_domain = knowledgeDomain;
      if (!Object.hasOwn(task, 'payload')) task.payload = {};
      if (task.payload && typeof task.payload === 'object' && !Array.isArray(task.payload)) task.payload.last_run = result;
      delete task._rev;
      await tx.put('research_tasks', task);
    }
    const found = await findRunByCommand(tx, commandId);
    const run = found ?? {
      id: `research_run_${completedAtMs}`,
      task_id: taskId,
      status: 'queued',
      command_id: commandId,
      task_queue_id: taskQueueId,
      identified_count: 0,
      accepted_count: 0,
      used_count: 0,
      payload: {},
      created_at_ms: completedAtMs,
      updated_at_ms: completedAtMs,
    };
    Object.assign(run, {
      task_id: taskId,
      status: 'completed',
      command_id: commandId,
      updated_at_ms: completedAtMs,
      identified_count: evidence.identified_count,
      accepted_count: evidence.accepted_count,
      used_count: evidence.used_count,
    });
    if (taskQueueId) run.task_queue_id = taskQueueId;
    if (!Object.hasOwn(run, 'payload')) run.payload = {};
    if (run.payload && typeof run.payload === 'object' && !Array.isArray(run.payload)) run.payload.result = result;
    delete run._rev;
    await tx.put('research_runs', run);
  });
  // CAP-3b #844: agent.command.settle ist KEIN OS-Primitiv (Befehls-Lebenszyklus). Der Ausgangsbefehl
  // research.systematic.run ist mit dem Einreihen beendet; Ergebnis und Status trägt der Rückruf-Befehl
  // (Rückgabewert bzw. geworfener Fehler) und der Lauf in research_runs/research_tasks (live).
  return result;
}

export async function persistFailure(ctx, { commandId, command, message, failedAtMs }) {
  await ctx.transaction(async (tx) => {
    const taskId = researchTaskId(command);
    const lastError = { command_id: commandId, message, failed_at_ms: failedAtMs };
    if (taskId) {
      const task = await tx.get('research_tasks', taskId);
      if (task) {
        task.status = 'failed';
        task.updated_at_ms = failedAtMs;
        if (!Object.hasOwn(task, 'payload')) task.payload = {};
        if (task.payload && typeof task.payload === 'object' && !Array.isArray(task.payload)) task.payload.last_error = lastError;
        delete task._rev;
        await tx.put('research_tasks', task);
      }
    }
    const run = await findRunByCommand(tx, commandId);
    if (run) {
      run.status = 'failed';
      run.updated_at_ms = failedAtMs;
      if (!Object.hasOwn(run, 'payload')) run.payload = {};
      if (run.payload && typeof run.payload === 'object' && !Array.isArray(run.payload)) run.payload.error = lastError;
      delete run._rev;
      await tx.put('research_runs', run);
    }
  });
  // CAP-3b #844: kein agent.command.settle – der Aufrufer wirft den Fehler weiter (Befehls-Lebenszyklus
  // des Rückrufs), der Fehlstand steht in research_runs/research_tasks.
}
