// Agentenläufe der threads-App (Kern §16.30, Strang SKL-A2).
//
// threads startet Agentenarbeit nicht mehr über den Kern-Befehl `business_os.chat.task`, sondern selbst über
// ctx.cap.agent.task.enqueue mit Rückruf-Befehlen der eigenen App:
//   threads.ai.request            → on_complete threads.ai.complete            / on_failed threads.ai.failed
//   threads.ctox_approval.approve → on_complete threads.ctox_approval.complete / on_failed threads.ctox_approval.failed
// Der Core schreibt nach Taskende genau einen Befehl {type, payload:{task_id, status, result, error?, workspace_refs?}};
// die Rückruf-Handler schreiben die Antwort als Nachricht in den Thread (live bei allen Clients) und sind idempotent
// (Doppelzustellung ändert nichts). Abbruch kommt als on_failed mit status 'cancelled'.
//
// Die Anweisung entsteht aus dem Template prompts/chat-task.md, einer wörtlichen Übernahme des Kern-Prompts für
// Befehls-Tasks (ref: store.rs:27009-27097 @ 4794af03d, command_prompt; Grenzen :203-206). Damit verschwindet die
// threads-Sonderbehandlung aus dem Core (command_prompt für business_os.chat.task aus threads, Chat-Projektion R7).
import { renderPrompt } from 'ctox:shared/prompt-template.js';
import {
  clone, fail, firstStringField, isObject, loadRecord, payloadOf, refreshThreadStates, slugPart, truncate,
  upsertMessage, upsertNotifications, upsertRecord, upsertThreadStatusDelta, valueString,
} from './_lib.js';
import { approvalCallbackEntry, dispatchApprovalCallback } from './_approval_targets.js';

export const AI_COMPLETE = 'threads.ai.complete';
export const AI_FAILED = 'threads.ai.failed';
export const APPROVAL_COMPLETE = 'threads.ctox_approval.complete';
export const APPROVAL_FAILED = 'threads.ctox_approval.failed';
export const CHAT_TASK_TEMPLATE = 'chat-task';

// ref: store.rs:203-206 (BUSINESS_OS_QUEUE_PROMPT_MAX_CHARS, …_JSON_PREVIEW_CHARS, …_INSTRUCTION_MAX_CHARS)
const PROMPT_MAX_CHARS = 96_000;
const JSON_PREVIEW_CHARS = 32_000;
const INSTRUCTION_MAX_CHARS = 8_000;

/** Pseudo-Befehl für Nachrichten, die der Agent schreibt (Autor CTOX). */
export const CTOX_AUTHOR = Object.freeze({ actor: { id: 'ctox', display_name: 'CTOX', role: 'system', is_admin: false } });

// ref: store.rs:27126-27148 command_prompt_sensitive_key
function sensitiveKey(key) {
  const k = String(key).trim().toLowerCase().replace(/-/g, '_');
  return ['authorization', 'proxy_authorization', 'cookie', 'set_cookie', 'password', 'passphrase', 'secret', 'api_key',
    'private_key', 'capability_token', 'access_token', 'refresh_token', 'id_token'].includes(k)
    || ['_token', '_password', '_passphrase', '_secret', '_api_key', '_private_key'].some((s) => k.endsWith(s));
}

/** ref: store.rs:27108-27125 sanitize_command_prompt_preview (Geheimnis-Schlüssel tief entfernen). */
export function sanitizePreview(value) {
  if (Array.isArray(value)) return value.map(sanitizePreview);
  if (isObject(value)) {
    const out = {};
    for (const [k, v] of Object.entries(value)) if (!sensitiveKey(k)) out[k] = sanitizePreview(v);
    return out;
  }
  return value;
}

const chars = (s) => [...s];

/** ref: store.rs:27708-27722 prompt_json_preview (ohne dedupe_repeated_long_strings). */
export function jsonPreview(value, maxChars = JSON_PREVIEW_CHARS) {
  const raw = JSON.stringify(value ?? {}, null, 2) ?? '{}';
  const all = chars(raw);
  if (all.length <= maxChars) return raw;
  const preview = all.slice(0, Math.max(0, maxChars - 160)).join('');
  return `${preview}\n... truncated ${all.length - chars(preview).length} chars; full JSON is stored on the Business OS command record ...`;
}

/** ref: store.rs:27724-27734 truncate_text_preserve. */
function truncatePreserve(text, maxChars) {
  const all = chars(text);
  if (all.length <= maxChars) return text;
  return `${all.slice(0, Math.max(0, maxChars - 80)).join('')}\n... truncated; full text is stored on the Business OS command record ...`;
}

/**
 * Anweisung für einen Agenten-Task aus prompts/chat-task.md (ref: store.rs:27009 command_prompt, allgemeiner Zweig).
 * instruction > 8 000 Zeichen → invalid (ref: store.rs:26999-27006), nie still gekürzt.
 */
export async function chatTaskInstruction(ctx, { instruction, commandId, module, type, recordId, payload, clientContext }) {
  const text = typeof instruction === 'string' && instruction.trim() !== '' ? instruction.trim() : 'Execute this Business OS automation through CTOX.';
  const count = chars(text).length;
  if (count > INSTRUCTION_MAX_CHARS) {
    fail('invalid', `business_command_instruction_too_large: selected instruction has ${count} characters, maximum ${INSTRUCTION_MAX_CHARS}; split the request into bounded commands instead of executing a shortened instruction`);
  }
  const preview = isObject(payload) ? clone(payload) : {};
  delete preview.instruction;
  delete preview.prompt;
  const rendered = await renderPrompt(ctx, CHAT_TASK_TEMPLATE, {
    instruction: text,
    command_id: commandId ?? '',
    module: module ?? '',
    type: type ?? '',
    record_id: recordId ?? '',
    payload_json: jsonPreview(sanitizePreview(preview)),
    context_json: jsonPreview(sanitizePreview(isObject(clientContext) ? clientContext : {})),
  });
  return truncatePreserve(rendered.replace(/\n$/, ''), PROMPT_MAX_CHARS);
}

// ---- Rückruf-Befehle ------------------------------------------------------------------------------------

/**
 * Rückruf-Befehl prüfen und lesen: {taskId, commandId, status, result, error, workspaceRefs}. Agenten-Task-Rückruf
 * (§16.30) trägt task_id; Befehls-Rückruf (§16.39 Nr. 3, nur mit allowCommandId) darf statt dessen command_id tragen.
 */
export function callbackOf(command, expected, { allowCommandId = false } = {}) {
  const module = firstStringField(command, ['module']) ?? firstStringField(command, ['app_id']);
  if (module !== 'threads') fail('invalid', 'threads commands require module=threads');
  const p = payloadOf(command);
  const taskId = typeof p.task_id === 'string' ? p.task_id.trim() : '';
  const commandId = allowCommandId && typeof p.command_id === 'string' ? p.command_id.trim() : '';
  if (taskId === '' && commandId === '') fail('invalid', allowCommandId ? 'task_id or command_id is required' : 'task_id is required');
  const raw = typeof p.status === 'string' ? p.status.trim().toLowerCase() : '';
  const status = expected === 'completed' ? 'completed' : (raw === 'cancelled' || raw === 'canceled' ? 'cancelled' : 'failed');
  return {
    taskId,
    commandId,
    status,
    result: p.result ?? null,
    error: p.error ?? null,
    workspaceRefs: Array.isArray(p.workspace_refs) ? p.workspace_refs.filter((x) => typeof x === 'string') : [],
  };
}

/** Antworttext eines Agentenlaufs (die Antwort war im Core reply_text, store.rs:11088 process_business_chat_reply). */
export function resultText(result) {
  if (typeof result === 'string') return result.trim();
  if (isObject(result)) {
    const text = firstStringField(result, ['text', 'reply_text', 'reply', 'answer', 'message', 'summary', 'output']);
    if (text !== undefined) return text;
    return Object.keys(result).length ? JSON.stringify(result, null, 2) : '';
  }
  if (result === null || result === undefined) return '';
  return String(result);
}

export function errorText(error) {
  if (typeof error === 'string') return error.trim();
  if (isObject(error)) return firstStringField(error, ['message', 'error', 'code']) ?? JSON.stringify(error);
  return '';
}

/**
 * Text einer Abschluss-/Fehler-Nachricht. Präfixe wörtlich aus ctox_status_body (ref: threads.rs:4288-4309,
 * _projection.ctox_relevance.js ctoxStatusBody), damit die Timeline Fehler wie bisher als Ereigniszeile zeigt.
 */
export function outcomeBody(status, { text = '', error = '', prompt = '' } = {}) {
  if (status === 'completed') return text !== '' ? text : ['CTOX Arbeit abgeschlossen.', prompt].map((p) => p.trim()).filter(Boolean).join('\n\n');
  const prefix = status === 'cancelled' ? 'CTOX Arbeit wurde beendet.' : 'CTOX Arbeit blockiert oder fehlgeschlagen.';
  return [prefix, error, prompt].map((p) => String(p || '').trim()).filter((p) => p !== '').join('\n\n');
}

const NOTIFICATION_TYPE = { completed: 'ctox_completed', failed: 'ctox_failed', cancelled: 'ctox_finished' };
const TERMINAL = new Set(['completed', 'failed', 'cancelled']);

/**
 * Ergebnis in den Thread schreiben (eine Transaktion): Nachricht `messageId` (completed → Art ai_result, Autor CTOX;
 * failed/cancelled → Ereigniszeile ctox_status), Benachrichtigung an `notifyUserIds`, Zähler neu berechnen.
 * Thread-Status bleibt, letzte Nachricht wird gesetzt.
 */
export async function writeOutcomeMessage(tx, { threadId, messageId, status, body, source, notifyUserIds, title, approvalRequestId = '', commandId = '', now }, projections) {
  const thread = await loadRecord(tx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  await upsertThreadStatusDelta(tx, threadId, valueString(thread, 'status') || 'open', messageId, now, 0, projections);
  await upsertMessage(tx, {
    threadId, messageId, kind: status === 'completed' ? 'ai_result' : 'ctox_status', command: CTOX_AUTHOR,
    targetUserIds: notifyUserIds, body, source, approvalRequestId, commandId, now,
  }, projections);
  await upsertNotifications(tx, {
    threadId, messageId, approvalRequestId, notificationType: NOTIFICATION_TYPE[status] ?? 'ctox_status',
    userIds: notifyUserIds, title: title || valueString(thread, 'title') || 'CTOX', body, source, now,
  }, projections);
  await refreshThreadStates(tx, threadId, now, projections);
}

// ---- KI-Auftrag (threads.ai.request) ------------------------------------------------------------------

export const aiTaskLinkId = (threadId, taskId) => `link_${slugPart(threadId)}_agent_tasks_${slugPart(taskId)}`;
export const aiResultMessageId = (taskId) => `msg_ai_${slugPart(taskId)}`;

/** Verknüpfung Thread ↔ Agenten-Task (eigene Collection, Schlüssel für den Rückruf, weil er die Payload nicht trägt). */
export async function aiTaskLinkForTask(store, taskId) {
  const rows = await store.read('user_thread_links', { selector: { task_id: taskId, link_role: 'ai_task' }, limit: 2 });
  return (rows || []).find((row) => row && row._deleted !== true && row.is_deleted !== true) ?? null;
}

/** Abschluss eines KI-Auftrags (threads.ai.complete / .failed). */
export async function settleAiRequest(command, ctx, expected) {
  const cb = callbackOf(command, expected);
  const now = ctx.now();
  const projections = [];
  const outcome = await ctx.transaction(async (tx) => {
    const link = await aiTaskLinkForTask(tx, cb.taskId);
    if (!link) fail('not_found', `no AI request for task ${cb.taskId}`);
    if (TERMINAL.has(valueString(link, 'status'))) return { ignored: true, link };
    const threadId = valueString(link, 'thread_id');
    const messageId = aiResultMessageId(cb.taskId);
    const body = outcomeBody(cb.status, { text: resultText(cb.result), error: errorText(cb.error) });
    const requester = valueString(link, 'requester_user_id');
    await writeOutcomeMessage(tx, {
      threadId, messageId, status: cb.status, body, source: isObject(link.context) ? link.context : {},
      notifyUserIds: requester ? [requester] : [], title: '', commandId: valueString(link, 'command_id'), now,
    }, projections);
    const next = clone(link);
    next.status = cb.status;
    next.result_message_id = messageId;
    next.finished_at_ms = now;
    if (cb.status !== 'completed') next.error = errorText(cb.error);
    if (cb.workspaceRefs.length) next.workspace_refs = cb.workspaceRefs;
    await upsertRecord(tx, 'user_thread_links', link.id, now, next);
    projections.push({ collection: 'user_thread_links', record_id: link.id });
    return { ignored: false, link: next, messageId };
  });
  if (outcome.ignored) return { ok: true, status: 'ignored', task_id: cb.taskId, thread_id: valueString(outcome.link, 'thread_id') };
  return { ok: true, status: cb.status, task_id: cb.taskId, thread_id: valueString(outcome.link, 'thread_id'), message_id: outcome.messageId, projections: dedupe(projections) };
}

function dedupe(projections) {
  const seen = new Set();
  return projections.filter((p) => {
    const key = `${p.collection}/${p.record_id}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

// ---- Freigabe (threads.ctox_approval.*) ----------------------------------------------------------------

const CALLBACK_TYPE = /^[a-z0-9][a-z0-9-]*(\.[a-z0-9_]+)+$/;

/**
 * Rückruf der anfragenden App (derzeit keiner, FIX10): {on_complete, on_failed} als Befehlstypen im Namensraum der Quell-App
 * (source_context.module), und nur ein fester Eintrag aus APPROVAL_CALLBACKS (§16.40, _approval_targets.js). threads
 * leitet das Ergebnis der freigegebenen Arbeit genau einmal dorthin weiter. null = kein Rückruf.
 */
export function approvalCallback(payload, source) {
  const raw = payload?.callback;
  if (raw === undefined || raw === null) return null;
  if (!isObject(raw)) fail('invalid', 'callback must be an object');
  const app = typeof source?.module === 'string' ? source.module.trim() : '';
  if (app === '' || app === 'threads') fail('invalid', 'callback requires a source_context.module of another app');
  const out = { app_id: app };
  for (const key of ['on_complete', 'on_failed']) {
    const type = typeof raw[key] === 'string' ? raw[key].trim() : '';
    if (type === '') fail('invalid', `callback.${key} is required`);
    if (!CALLBACK_TYPE.test(type) || !type.startsWith(`${app}.`)) fail('invalid', `callback.${key} must be a command type of app ${app}`);
    out[key] = type;
  }
  if (!approvalCallbackEntry(app, out.on_complete, out.on_failed)) {
    fail('invalid', `callback ${out.on_complete}/${out.on_failed} of app ${app} is not a declared approval callback of threads (§16.40)`);
  }
  return out;
}

export const approvalResultMessageId = (approvalId) => `msg_approval_result_${slugPart(approvalId)}`;

/**
 * Abschluss der freigegebenen Arbeit (Rückruf eines Agenten-Tasks oder terminales Ergebnis eines synchron
 * ausgeführten Zielbefehls, z. B. ctox.coding.turn). Idempotent über approval.execution_status: nur eine Freigabe
 * im Stand approved ohne terminalen Ausführungsstand wird abgeschlossen; jede weitere Zustellung → ignored.
 */
export async function settleApproval(ctx, approvalId, { status, result = null, error = null, taskId = '', commandId = '' }) {
  const now = ctx.now();
  const projections = [];
  const outcome = await ctx.transaction(async (tx) => {
    const approval = await loadRecord(tx, 'ctox_task_approval_requests', approvalId);
    if (!approval) fail('not_found', `approval request ${approvalId} not found`);
    if (valueString(approval, 'status') !== 'approved' || TERMINAL.has(valueString(approval, 'execution_status'))) {
      return { ignored: true, approval };
    }
    const threadId = valueString(approval, 'thread_id');
    const messageId = approvalResultMessageId(approvalId);
    const text = resultText(result);
    const err = errorText(error);
    const body = outcomeBody(status, { text, error: err, prompt: status === 'completed' ? valueString(approval, 'prompt') : '' });
    const requester = valueString(approval, 'requester_user_id');
    const source = isObject(approval.source_context) ? approval.source_context : {};
    await writeOutcomeMessage(tx, {
      threadId, messageId, status, body, source, notifyUserIds: requester ? [requester] : [], title: '',
      approvalRequestId: approvalId, commandId: valueString(approval, 'approved_command_id'), now,
    }, projections);
    const next = clone(approval);
    next.execution_status = status;
    next.execution_finished_at_ms = now;
    next.result_message_id = messageId;
    next.result_text = truncate(text, 4000);
    next.execution_error = err;
    if (taskId !== '' && valueString(next, 'approved_task_id') === '') next.approved_task_id = taskId;
    await upsertRecord(tx, 'ctox_task_approval_requests', approvalId, now, next);
    projections.push({ collection: 'ctox_task_approval_requests', record_id: approvalId });
    return { ignored: false, approval: next, text, err, messageId };
  });
  if (outcome.ignored) return { ok: true, status: 'ignored', approval_request_id: approvalId };
  const forwarded = await forwardApprovalCallback(ctx, outcome.approval, { status, text: outcome.text, error: outcome.err, taskId, commandId });
  return {
    ok: true, status, approval_request_id: approvalId, thread_id: valueString(outcome.approval, 'thread_id'),
    message_id: outcome.messageId, forwarded, projections: dedupe(projections),
  };
}

/** Ergebnis an die anfragende App weiterleiten (genau einmal, nach dem Schreiben). */
async function forwardApprovalCallback(ctx, approval, { status, text, error, taskId, commandId }) {
  const cb = isObject(approval.callback) ? approval.callback : null;
  if (!cb) return null;
  const type = status === 'completed' ? cb.on_complete : cb.on_failed;
  if (typeof type !== 'string' || type === '') return null;
  const approvalId = valueString(approval, 'approval_request_id');
  const recordId = valueString(approval, 'target_record_id');
  const payload = {
    approval_request_id: approvalId,
    status,
    result_text: text,
    error,
    task_id: taskId !== '' ? taskId : valueString(approval, 'approved_task_id'),
    command_id: commandId !== '' ? commandId : valueString(approval, 'approved_command_id'),
    target_module: valueString(approval, 'target_module'),
    target_record_id: recordId,
    target_command_type: valueString(approval, 'target_command_type'),
    source_context: isObject(approval.source_context) ? clone(approval.source_context) : {},
  };
  const accepted = await dispatchApprovalCallback(ctx, type, payload, {
    id: `cmd_callback_${slugPart(approvalId)}_${status}`, record_id: recordId,
    client_context: { action: 'threads.ctox_approval.callback', source_module: 'threads', module: cb.app_id, app_id: cb.app_id, approval_request_id: approvalId },
  });
  // Altbestand mit einem Rückruf außerhalb der festen Liste: nichts senden (§16.40).
  if (accepted === null) ctx.log?.('warn', 'approval callback not declared; not forwarded', { approval_request_id: approvalId, type });
  return { type, accepted: accepted ?? null };
}

async function approvalBy(store, field, value) {
  if (value === '') return null;
  const rows = await store.read('ctox_task_approval_requests', { selector: { [field]: value }, limit: 2 });
  return (rows || []).find((row) => row && row._deleted !== true) ?? null;
}

/** Freigabe zu einem Task finden (Agenten-Task-Rückruf trägt nur task_id). */
export async function approvalForTask(store, taskId) {
  return approvalBy(store, 'approved_task_id', taskId);
}

/**
 * threads.ctox_approval.complete / .failed: Rückruf eines Agenten-Tasks (task_id) oder des freigegebenen Befehls an
 * eine feste Ziel-App (command_id = approved_command_id, §16.39 Nr. 3). Idempotent über settleApproval.
 */
export async function settleApprovalCallback(command, ctx, expected) {
  const cb = callbackOf(command, expected, { allowCommandId: true });
  const approval = (await approvalForTask(ctx, cb.taskId)) ?? (await approvalBy(ctx, 'approved_command_id', cb.commandId));
  if (!approval) fail('not_found', `no approval request for ${cb.taskId !== '' ? `task ${cb.taskId}` : `command ${cb.commandId}`}`);
  return settleApproval(ctx, valueString(approval, 'approval_request_id') || approval.id, {
    status: cb.status, result: cb.result, error: cb.error, taskId: cb.taskId, commandId: cb.commandId,
  });
}
