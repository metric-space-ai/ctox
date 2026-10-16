// Rückrufe der Agentenaufträge aus Knöpfen der Outbound-App (Strang SKL-O, Kern §16.30 Nr. 3).
//
// Der Kern schreibt genau einmal je task_id einen Befehl {type: on_complete|on_failed, payload:{task_id, status,
// result, error?, workspace_refs?}} an die App. Die Rückruf-Handler lesen die Aufgabe (agent.task.get), nehmen die
// eigenen Bezüge (campaign_id bzw. pipeline_id/contact_index) aus deren payload und werten die Antwort aus.
// Antwortauswertung wie im Core (ref: store.rs:23796 extract_json_object – erstes vollständiges JSON-Objekt).
import { fail, isObject } from './outbound.js';

export const TASK_FAILED = ['failed', 'blocked', 'cancelled'];

/** Rückruf-Payload prüfen und die Aufgabe lesen. `expect`: 'completed' (on_complete) oder 'failed' (on_failed). */
export async function loadTaskCallback(command, ctx, label, expect) {
  const payload = isObject(command.payload) ? command.payload : {};
  const taskId = typeof payload.task_id === 'string' ? payload.task_id.trim() : '';
  if (taskId === '') fail('invalid', `${label}: task_id is required`);
  const status = typeof payload.status === 'string' ? payload.status : '';
  if (expect === 'completed' && status !== 'completed') fail('invalid', `${label}: task status ${status || '-'} is not completed (on_failed handles it)`);
  if (expect === 'failed' && !TASK_FAILED.includes(status)) fail('invalid', `${label}: task status ${status || '-'} is not a failure`);
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  const taskPayload = isObject(task?.payload) ? task.payload : null;
  if (!taskPayload) fail('not_found', `${label}: task ${taskId} not found`);
  return { taskId, status, taskPayload, payload };
}

/** Text einer Agentenantwort (result als Text, {text} oder Objekt). */
export function resultText(result) {
  if (typeof result === 'string') return result;
  if (typeof result?.text === 'string') return result.text;
  return result == null ? '' : JSON.stringify(result);
}

// ref: store.rs:23796 extract_json_object – erstes vollständiges JSON-Objekt einer Agentenantwort.
export function extractJsonObject(text) {
  const trimmed = String(text ?? '').trim();
  try {
    const value = JSON.parse(trimmed);
    if (isObject(value)) return value;
  } catch { /* weiter mit Klammerzählung */ }
  const start = trimmed.indexOf('{');
  if (start < 0) return undefined;
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let i = start; i < trimmed.length; i += 1) {
    const ch = trimmed[i];
    if (inString) {
      if (escaped) escaped = false;
      else if (ch === '\\') escaped = true;
      else if (ch === '"') inString = false;
      continue;
    }
    if (ch === '"') inString = true;
    else if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) {
        try {
          const value = JSON.parse(trimmed.slice(start, i + 1));
          return isObject(value) ? value : undefined;
        } catch {
          return undefined;
        }
      }
    }
  }
  return undefined;
}

/** Antwort als JSON-Objekt: result selbst (Objekt ohne text) oder erstes Objekt im Antworttext. */
export function replyObject(result) {
  if (isObject(result) && typeof result.text !== 'string') return result;
  return extractJsonObject(resultText(result));
}

/** Fehlermeldung eines on_failed-Rückrufs: error.message, sonst error/result.error/result.message, sonst „task <status>“. */
export function failureMessage(payload) {
  const hit = [payload?.error?.message, payload?.error, payload?.result?.error, payload?.result?.message, payload?.status_note]
    .find((value) => typeof value === 'string' && value.trim() !== '');
  return hit ? hit.trim() : `task ${payload?.status ?? 'failed'}`;
}
