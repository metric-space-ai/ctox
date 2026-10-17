// Kontext-Chat der ctox-App (Shell v4, Strang SKL-D): die zwei Standardbefehle, die jede App über den Baustein
// ctox:shared/context-chat.js (api.command) an CTOX richtet, und ihr Rückweg in die Chat-Collection der ctox-App.
//
//   ctox.chat.task            Kontext-Auftrag (Modus data|ask), Template prompts/chat-data.md | chat-ask.md, kein Skill
//   ctox.app.modify.request   App ändern, Template prompts/app-modify.md, System-Skill business-os-app-module-development
//   ctox.chat.task.complete   on_complete beider Befehle: Antwort als Nachricht in business_chats (live bei allen Clients)
//   ctox.chat.task.failed     on_failed beider Befehle (auch Abbruch, status 'cancelled'): Fehlschlag als Nachricht
//
// Vorher (Bestandsaufnahme F #30/#31/#33): 8 Kopien des Kontextmenüs plus mail „An CTOX übergeben“ und ctox
// „Aufgaben aus Datei“/„Aufgabe fortsetzen“ sendeten rohe business_os.chat.task / ctox.business_os.app.modify ohne
// App-Handler; Prompt, Skill und Rückweg lagen im Core:
//   // ref: store.rs:26700-26790 create_ctox_queue_task (Titel, thread_key business-os/<module>)
//   // ref: store.rs:27009-27100 command_prompt (Anweisung + Befehlsblock + Payload-/Kontext-JSON, sensible Schlüssel raus)
//   // ref: store.rs:27354-27390 business_os_app_command_prompt (App-Ziel-Block, Anfragezusammenfassung)
//   // ref: store.rs:27418-27484 required_skill_names / is_business_os_app_module_command → business-os-app-module-development
//   // ref: store.rs:27490-27560 business_os_app_command_target_prompt_block / _target_metadata
//   // ref: store.rs:6900-6960 + business-chat.js (R7 Chat-Projektion: Antwort als role 'ctox' mit replyFor/failureFor)
// Jetzt: Templates im Paket, Rückruf als eigener ctox-Befehl, Antwort in die eigene Collection business_chats.
import { renderPrompt } from 'ctox:shared/prompt-template.js';

export const CHAT_TASK = 'ctox.chat.task';
export const APP_MODIFY_REQUEST = 'ctox.app.modify.request';
export const CHAT_TASK_COMPLETE = 'ctox.chat.task.complete';
export const CHAT_TASK_FAILED = 'ctox.chat.task.failed';
export const APP_MODULE_SKILL = 'business-os-app-module-development'; // ref: store.rs:27418 BUSINESS_OS_APP_MODULE_SKILL_NAME
export const CHATS = 'business_chats';

const TEXT_MAX_CHARS = 8_000; // ref: store.rs:206 BUSINESS_OS_QUEUE_PROMPT_INSTRUCTION_MAX_CHARS
const JSON_PREVIEW_CHARS = 32_000; // ref: store.rs:205 BUSINESS_OS_QUEUE_PROMPT_JSON_PREVIEW_CHARS
const REPLY_MAX_CHARS = 64_000;
const SELECTION_MAX_CHARS = 1_000;
const ID_MAX = 128; // business_chats.id maxLength
const APP_ID = /^[a-z0-9][a-z0-9_-]{0,63}$/;
const MODIFY_ROLES = new Set(['owner', 'admin', 'chef']);
const MODIFY_PERMISSION = 'apps.source.edit';

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const text = (v) => (typeof v === 'string' ? v.trim() : '');
const chars = (v) => [...v].length;
const fail = (code, message) => ({ code, message });

// ref: store.rs:27126-27149 command_prompt_sensitive_key
const SENSITIVE = new Set(['authorization', 'proxy_authorization', 'cookie', 'set_cookie', 'password', 'passphrase', 'secret',
  'api_key', 'private_key', 'capability_token', 'access_token', 'refresh_token', 'id_token']);
const SENSITIVE_SUFFIX = ['_token', '_password', '_passphrase', '_secret', '_api_key', '_private_key'];
function sensitive(key) {
  const k = key.trim().toLowerCase().replaceAll('-', '_');
  return SENSITIVE.has(k) || SENSITIVE_SUFFIX.some((s) => k.endsWith(s));
}
export function sanitize(value) {
  if (Array.isArray(value)) return value.map(sanitize);
  if (!isObject(value)) return value;
  return Object.fromEntries(Object.entries(value).filter(([k]) => !sensitive(k)).map(([k, v]) => [k, sanitize(v)]));
}

// ref: store.rs:27708-27722 prompt_json_preview
export function jsonPreview(value, max = JSON_PREVIEW_CHARS) {
  const raw = JSON.stringify(value ?? {}, null, 2);
  const all = [...raw];
  if (all.length <= max) return raw;
  const preview = all.slice(0, max - 160).join('');
  return `${preview}\n... truncated ${all.length - [...preview].length} chars; full JSON is stored on the Business OS command record ...`;
}

function clip(value, max) {
  const all = [...value];
  return all.length <= max ? value : `${all.slice(0, max).join('')}…`;
}

/** Payload des Bausteins prüfen: {source_app, record_ref, selection, text, mode, title?, thread_key?, context?}. */
export function contextRequest(command, { allowedModes }) {
  const p = isObject(command.payload) ? command.payload : {};
  const sourceApp = text(p.source_app);
  if (!APP_ID.test(sourceApp)) throw fail('invalid', 'source_app is required (app id)');
  const message = text(p.text);
  if (!message) throw fail('invalid', 'text is required');
  if (chars(message) > TEXT_MAX_CHARS) {
    throw fail('invalid', `business_command_instruction_too_large: text has ${chars(message)} characters, maximum ${TEXT_MAX_CHARS}`);
  }
  const mode = text(p.mode) || allowedModes[0];
  if (!allowedModes.includes(mode)) throw fail('invalid', `mode ${mode} is not allowed for ${command.type}`);
  const recordRef = text(p.record_ref) || text(command.record_id) || sourceApp;
  const chatId = text(p.chat_id) || text(command.id);
  if (!chatId || chatId.length > ID_MAX) throw fail('invalid', 'chat id missing or longer than 128');
  return {
    sourceApp,
    recordRef,
    selection: clip(text(p.selection), SELECTION_MAX_CHARS),
    text: message,
    mode,
    title: text(p.title),
    threadKey: text(p.thread_key) || `business-os/${sourceApp}`, // ref: store.rs:26936 command_queue_thread_key
    context: isObject(p.context) ? p.context : {},
    chatId,
  };
}

/** App ändern ist serverseitig zu entscheiden (AGENTS.md: kein reines UI-Gate). Rolle owner/admin/chef oder Recht. */
export function assertMayModifyApp(command) {
  const actor = isObject(command.actor) ? command.actor : {};
  const role = text(actor.role).toLowerCase().replace(/^business_os_/, '');
  const permissions = Array.isArray(actor.permissions) ? actor.permissions : [];
  if (MODIFY_ROLES.has(role) || permissions.includes(MODIFY_PERMISSION)) return;
  throw fail('forbidden', `${APP_MODIFY_REQUEST} requires role admin/chef or permission ${MODIFY_PERMISSION}`);
}

function promptPayload(req) {
  return sanitize({ source_app: req.sourceApp, record_ref: req.recordRef, selection: req.selection, mode: req.mode, context: req.context });
}

// ref: store.rs:27530-27555 business_os_app_command_target_metadata (Standard install_target runtime-installed-module)
function appTarget(req) {
  const installTarget = text(req.context.install_target) || 'runtime-installed-module';
  return {
    module_id: req.sourceApp,
    install_target: installTarget,
    app_directory: installTarget === 'runtime-installed-module'
      ? `runtime/business-os/installed-modules/${req.sourceApp}`
      : `src/apps/business-os/modules/${req.sourceApp}`,
    mode_flag: installTarget === 'runtime-installed-module' ? '--installed' : '--source',
  };
}

// ref: store.rs:27560-27630 business_os_app_command_request_summary (hier: app_title aus title, Rest fehlt im Payload)
function requestSummary(req) {
  return req.title ? `- app_title: ${clip(req.title, 240)}` : '- no extra summary fields';
}

async function upsertChatTurn(ctx, req, command, taskId, now) {
  await ctx.transaction(async (tx) => {
    const existing = await tx.get(CHATS, req.chatId);
    const chat = existing ? { ...existing } : {
      id: req.chatId,
      title: req.title || `${req.sourceApp} · ${req.recordRef}`,
      owner_user_id: text(command.actor?.id),
      messages: [],
      open: true,
      createdAt: now,
    };
    delete chat._rev;
    const messages = Array.isArray(chat.messages) ? [...chat.messages] : [];
    const userMessageId = `user_${command.id}`;
    if (!messages.some((m) => m?.id === userMessageId)) {
      messages.push({ id: userMessageId, role: 'user', text: req.text, createdAt: now, commandId: command.id, taskId, sourceApp: req.sourceApp, mode: req.mode });
    }
    Object.assign(chat, {
      messages,
      lastTrackingId: taskId,
      tracking_active: true,
      tracking_status: 'queued',
      tracking_command_id: command.id,
      tracking_task_id: taskId,
      tracking_message_id: userMessageId,
      updated_at_ms: now,
    });
    await tx.put(CHATS, chat);
  });
}

/** ctox.chat.task / ctox.app.modify.request → Template → agent.task.enqueue → Chat-Turn in business_chats. */
export async function enqueueContextTask(command, ctx) {
  const appModify = command.type === APP_MODIFY_REQUEST;
  if (!appModify && command.type !== CHAT_TASK) throw fail('invalid', `unknown context chat command ${command.type}`);
  const req = contextRequest(command, { allowedModes: appModify ? ['app'] : ['data', 'ask'] });
  if (appModify) assertMayModifyApp(command);
  const common = { text: req.text, command_id: command.id, command_type: command.type, source_app: req.sourceApp, record_id: req.recordRef };
  const instruction = appModify
    ? await renderPrompt(ctx, 'app-modify', { ...common, ...appTarget(req), request_summary: requestSummary(req) })
    : await renderPrompt(ctx, req.mode === 'ask' ? 'chat-ask' : 'chat-data', {
      ...common,
      payload_json: jsonPreview(promptPayload(req)),
      client_context_json: jsonPreview(sanitize(isObject(command.client_context) ? command.client_context : {})),
    });
  const title = req.title || `${req.sourceApp} · ${req.recordRef}`;
  const task = await ctx.cap.agent.task.enqueue({
    title,
    instruction: instruction.replace(/\s+$/, ''),
    skill: appModify ? APP_MODULE_SKILL : undefined, // Kontext-Auftrag ohne Skill (Bestand: keiner)
    thread_key: req.threadKey,
    payload: { chat_id: req.chatId, source_app: req.sourceApp, record_ref: req.recordRef, mode: req.mode, business_command_id: command.id, command_type: command.type },
    idempotency_key: command.id,
    on_complete: CHAT_TASK_COMPLETE,
    on_failed: CHAT_TASK_FAILED,
  });
  const taskId = isObject(task) && typeof task.task_id === 'string' ? task.task_id : '';
  if (!taskId) throw fail('unavailable', 'agent.task.enqueue returned no task_id');
  await upsertChatTurn(ctx, req, command, taskId, ctx.now());
  return { ok: true, command_id: command.id, task_id: taskId, chat_id: req.chatId, status: 'queued' };
}

export function callbackOf(command) {
  const p = isObject(command.payload) ? command.payload : {};
  const taskId = text(p.task_id);
  if (!taskId) throw fail('invalid', 'task_id is required');
  const error = isObject(p.error) ? p.error : (typeof p.error === 'string' ? { message: p.error } : null);
  return { taskId, status: text(p.status), result: p.result ?? null, error };
}

/** Chat des Auftrags: chat_id aus dem Task-Payload, sonst über tracking_task_id. */
async function chatIdFor(ctx, taskId) {
  let task = null;
  try { task = await ctx.cap.agent.task.get({ task_id: taskId }); } catch (error) { if (error?.code !== 'not_found') throw error; }
  const fromTask = text(task?.payload?.chat_id);
  if (fromTask) return fromTask;
  const [hit] = await ctx.read(CHATS, { selector: { tracking_task_id: taskId }, limit: 1 });
  return text(hit?.id);
}

export function replyText(result) {
  if (typeof result === 'string') return result.trim();
  if (!isObject(result)) return '';
  for (const key of ['text', 'reply', 'message', 'summary']) {
    if (typeof result[key] === 'string' && result[key].trim()) return result[key].trim();
  }
  return '';
}

async function appendCallbackMessage(ctx, taskId, build) {
  const chatId = await chatIdFor(ctx, taskId);
  if (!chatId) return { ok: false, task_id: taskId, reason: 'chat_missing' };
  let outcome = null;
  await ctx.transaction(async (tx) => {
    const current = await tx.get(CHATS, chatId);
    if (!current) { outcome = { ok: false, task_id: taskId, reason: 'chat_missing' }; return; }
    const messages = Array.isArray(current.messages) ? [...current.messages] : [];
    const { message, status } = build();
    // Doppelzustellung: je task_id höchstens eine Antwort- oder Fehlernachricht.
    if (messages.some((m) => m && (m.replyFor === taskId || m.failureFor === taskId))) {
      outcome = { ok: true, task_id: taskId, chat_id: chatId, duplicate: true };
      return;
    }
    messages.push(message);
    const next = { ...current, messages, updated_at_ms: ctx.now() };
    delete next._rev;
    if (current.tracking_task_id === taskId) Object.assign(next, { tracking_active: false, tracking_status: status });
    await tx.put(CHATS, next);
    outcome = { ok: true, task_id: taskId, chat_id: chatId, status };
  });
  return outcome;
}

/** ctox.chat.task.complete: Antwort des Agenten als Nachricht role 'ctox' (replyFor = task_id). */
export async function completeContextTask(command, ctx) {
  const cb = callbackOf(command);
  return appendCallbackMessage(ctx, cb.taskId, () => ({
    status: 'completed',
    message: {
      id: `reply_${cb.taskId}`,
      role: 'ctox',
      text: clip(replyText(cb.result), REPLY_MAX_CHARS),
      replyFor: cb.taskId,
      taskId: cb.taskId,
      status: 'completed',
      createdAt: ctx.now(),
    },
  }));
}

/** ctox.chat.task.failed: Fehlschlag oder Abbruch als Nachricht role 'ctox' (failureFor = task_id). */
export async function failContextTask(command, ctx) {
  const cb = callbackOf(command);
  const status = cb.status === 'cancelled' ? 'cancelled' : 'failed';
  return appendCallbackMessage(ctx, cb.taskId, () => ({
    status,
    message: {
      id: `failure_${cb.taskId}`,
      role: 'ctox',
      text: clip(text(cb.error?.message) || status, 2_000),
      failureFor: cb.taskId,
      taskId: cb.taskId,
      status,
      ...(text(cb.error?.code) ? { errorCode: text(cb.error.code) } : {}),
      createdAt: ctx.now(),
    },
  }));
}
