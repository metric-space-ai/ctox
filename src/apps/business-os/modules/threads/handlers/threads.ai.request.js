// threads.ai.request — Port von create_ai_request → create_ai_request_with_supervisor_turn(None)
// (threads.rs:2212-2313). Strang SKL-A2 (Kern §16.30): kein ctx.command('business_os.chat.task') mehr; der Handler
// reiht den Agenten-Task selbst ein (ctx.cap.agent.task.enqueue) und nennt eigene Rückruf-Befehle:
// threads.ai.complete schreibt die Antwort als Nachricht in den Thread, threads.ai.failed die Fehler-/Abbruchzeile.
// Anweisung aus prompts/chat-task.md (ref: store.rs:27009 command_prompt, bisher im Core für chat.task gebaut).
// Nicht portiert: create_supervisor_ai_request (:2220) – nativer Workjet-Supervisor-Pfad,
// kein threads-Befehlstyp (Aufrufer project_chats), bleibt im Core.
import {
  actorId, actorPayload, clone, fail, firstNonEmpty, firstStringField, isObject, loadRecord, payloadOf, preflight,
  requiredString, threadSourceContext, truncate, upsertRecord, valueString,
} from './_lib.js';
import { createMessage } from './threads.message.create.js';
import { AI_COMPLETE, AI_FAILED, aiTaskLinkId, chatTaskInstruction } from './_agent.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const payload = payloadOf(command);
  const goal = requiredString(payload, ['goal', 'prompt', 'instruction']);
  const delegatedPayload = isObject(command.payload) ? clone(command.payload) : {};
  delegatedPayload.kind = 'ai_request';
  delegatedPayload.body = goal;
  delegatedPayload.target_user_ids = [];
  const message = await createMessage(ctx, { ...command, type: 'threads.message.create', payload: delegatedPayload });
  const threadId = valueString(message, 'thread_id');
  // U-8: Rust prüft risk_class erst NACH dem Schreiben der Nachricht; beibehalten.
  const riskClass = firstStringField(payload, ['risk_class']) ?? 'internal';
  if (riskClass !== 'internal') fail('invalid', 'non-internal AI actions require a CTOX approval request');
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  const module = firstNonEmpty([valueString(thread, 'source_module'), 'ctox']);
  const recordId = valueString(thread, 'source_record_id');
  const source = threadSourceContext(thread);
  const title = `AI: ${truncate(goal, 80)}`;
  const threadKey = `business-os/threads/${threadId}`;
  // Payload wie bisher an business_os.chat.task (threads.rs:2286-2299), Vorschau im Prompt.
  const taskPayload = {
    title,
    user_message: goal,
    thread_key: threadKey,
    thread_id: threadId,
    risk_class: riskClass,
    context: source,
    request_message_id: valueString(message, 'message_id'),
    request_command_id: typeof command.id === 'string' ? command.id : '',
    requester_user_id: actorId(command),
    module,
    record_id: recordId,
  };
  const clientContext = {
    actor: actorPayload(command),
    action: 'threads.ai.request',
    source_module: 'threads',
    module,
    module_id: module,
    app_id: module,
  };
  const instruction = await chatTaskInstruction(ctx, {
    instruction: goal, commandId: taskPayload.request_command_id, module, type: 'threads.ai.request', recordId,
    payload: taskPayload, clientContext,
  });
  const queued = await ctx.cap.agent.task.enqueue({
    title,
    instruction,
    thread_key: threadKey,
    payload: taskPayload,
    idempotency_key: `threads.ai.request:${taskPayload.request_command_id || valueString(message, 'message_id')}`,
    on_complete: AI_COMPLETE,
    on_failed: AI_FAILED,
  });
  const taskId = typeof queued?.task_id === 'string' ? queued.task_id : '';
  if (taskId === '') fail('unavailable', 'agent task enqueue did not return task_id');
  // Verknüpfung Thread ↔ Task: Schlüssel für den Rückruf (der nur task_id trägt).
  const now = ctx.now();
  const linkId = aiTaskLinkId(threadId, taskId);
  await upsertRecord(ctx, 'user_thread_links', linkId, now, {
    id: linkId,
    thread_id: threadId,
    source_module: 'ctox',
    source_record_type: 'agent_task',
    source_record_id: taskId,
    source_label: title,
    link_role: 'ai_task',
    link_type: 'agent_task',
    command_id: taskPayload.request_command_id,
    task_id: taskId,
    status: 'queued',
    requester_user_id: taskPayload.requester_user_id,
    request_message_id: taskPayload.request_message_id,
    deep_link: '',
    context: source,
    created_at_ms: now,
  });
  return { ok: true, thread_id: threadId, message, task_id: taskId, link_id: linkId };
}
