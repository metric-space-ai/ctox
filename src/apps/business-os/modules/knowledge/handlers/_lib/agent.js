// Agenten-Aufträge der Knowledge-App (Kern §16.30, Strang SKL-B2).
//
// Bis SKL-B2 hatten die 7 Typen ctox.knowledge.* keinen Handler: der Befehl lief über den generischen Weg
// Befehl → Queue-Task, die Anweisung stand als Literal in index.js (:2250, :2299, :2427, :2463, :2504), den Skill
// riet die Core-Heuristik (Phantom `knowledge`), und nach dem Turn gab es keinen typisierten Abschluss.
//   // ref: store.rs:26700-26790 create_ctox_queue_task (Titel, Priorität, thread_key, suggested_skill)
//   // ref: store.rs:27009-27100 command_prompt (Anweisung + Befehlsblock + Payload-/Kontext-JSON, sensible Schlüssel raus)
//   // ref: store.rs:27736-27756 suggested_skill_for_command (Typ enthält "knowledge" → Phantom `knowledge`)
//   // ref: store.rs:10312-10411 complete_business_command_from_queue_reply (kein Zweig für ctox.knowledge.* → nur
//   //      generischer Terminalübergang; Ergebnis = Antworttext)
// Jetzt: Template prompts/<name>.md, eigener Skill knowledge/knowledge-maintenance, Rückruf knowledge.task.complete /
// knowledge.task.failed. Den Auftrag spiegelt die eigene Collection knowledge_tasks (live bei allen Clients).
import { renderPrompt } from 'ctox:shared/prompt-template.js';

export const KNOWLEDGE_SKILL = 'knowledge/knowledge-maintenance';
export const TASK_COMPLETE = 'knowledge.task.complete';
export const TASK_FAILED = 'knowledge.task.failed';
export const TASKS = 'knowledge_tasks';

const JSON_PREVIEW_CHARS = 32_000; // ref: store.rs:205 BUSINESS_OS_QUEUE_PROMPT_JSON_PREVIEW_CHARS
const INSTRUCTION_MAX_CHARS = 8_000; // ref: store.rs:206 BUSINESS_OS_QUEUE_PROMPT_INSTRUCTION_MAX_CHARS
const DEFAULT_THREAD = 'business-os/knowledge'; // ref: store.rs:26936 command_queue_thread_key → business-os/<module>

export function invalid(message) {
  return { code: 'invalid', message };
}
const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const text = (v) => (typeof v === 'string' ? v.trim() : '');

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

// ref: store.rs:27708-27722 prompt_json_preview (pretty, gekappt mit Hinweis)
export function jsonPreview(value, max = JSON_PREVIEW_CHARS) {
  const raw = JSON.stringify(value ?? {}, null, 2);
  const chars = [...raw];
  if (chars.length <= max) return raw;
  const preview = chars.slice(0, max - 160).join('');
  return `${preview}\n... truncated ${chars.length - [...preview].length} chars; full JSON is stored on the Business OS command record ...`;
}

/**
 * Befehlstyp → Template und Titel. `title` ersetzt store.rs::command_title nur als Rückfall (UI liefert title).
 * `values` liefert typ-eigene Platzhalter (Runbook-Text, Nutzernachricht).
 */
export const KNOWLEDGE_COMMANDS = Object.freeze({
  'ctox.knowledge.document.modify': { prompt: 'document-modify' },
  'ctox.knowledge.runbook.modify': { prompt: 'runbook-modify' },
  'ctox.knowledge.runbook.execute': {
    prompt: 'runbook-execute',
    values: (p) => {
      const runbookInstruction = text(p.runbook_instruction);
      if (!runbookInstruction) throw invalid('runbook_instruction is required');
      return { runbook_instruction: runbookInstruction };
    },
    priority: 'normal',
  },
  'ctox.knowledge.book.create': { prompt: 'book-create' },
  'ctox.knowledge.book.import': { prompt: 'book-import' },
  'ctox.knowledge.book.export': { prompt: 'book-export' },
  'ctox.knowledge.chat': {
    prompt: 'chat',
    values: (p) => {
      const message = text(p.user_message);
      if (!message) throw invalid('user_message is required');
      return { user_message: message };
    },
  },
});

const PAYLOAD_NOT_IN_PROMPT = new Set(['record_id', 'module', 'client_context', 'instruction', 'prompt', 'runbook_instruction', 'user_message']);

function displayType(type) {
  return type.replaceAll('business_os.', '').replaceAll('ctox.', '').replace(/[_.-]/g, ' ');
}

/** Knopf → Befehl ctox.knowledge.* → Template füllen → agent.task.enqueue → knowledge_tasks (queued). */
export async function enqueueKnowledgeTask(command, ctx) {
  const spec = KNOWLEDGE_COMMANDS[command.type];
  if (!spec) throw invalid(`unknown knowledge command ${command.type}`);
  const p = isObject(command.payload) ? command.payload : {};
  const recordId = text(command.record_id) || text(p.record_id);
  const clientContext = isObject(command.client_context) ? command.client_context : (isObject(p.client_context) ? p.client_context : {});
  const typeValues = spec.values ? spec.values(p) : {};
  for (const [k, v] of Object.entries(typeValues)) {
    if ([...v].length > INSTRUCTION_MAX_CHARS) {
      throw invalid(`business_command_instruction_too_large: ${k} has ${[...v].length} characters, maximum ${INSTRUCTION_MAX_CHARS}`);
    }
  }
  const promptPayload = Object.fromEntries(Object.entries(p).filter(([k]) => !PAYLOAD_NOT_IN_PROMPT.has(k)));
  const instruction = (await renderPrompt(ctx, spec.prompt, {
    ...typeValues,
    command_id: command.id,
    command_type: command.type,
    record_id: recordId,
    payload_json: jsonPreview(sanitize(promptPayload)),
    client_context_json: jsonPreview(sanitize(clientContext)),
  })).replace(/\s+$/, '');
  const title = text(p.title) || `${displayType(command.type)} · ${recordId || 'new task'}`;
  const task = await ctx.cap.agent.task.enqueue({
    title,
    instruction,
    skill: KNOWLEDGE_SKILL,
    thread_key: text(p.thread_key) || DEFAULT_THREAD,
    priority: text(p.priority) || spec.priority || 'normal',
    payload: { ...sanitize(promptPayload), business_command_id: command.id, command_type: command.type, record_id: recordId },
    idempotency_key: command.id,
    on_complete: TASK_COMPLETE,
    on_failed: TASK_FAILED,
  });
  const taskId = isObject(task) && typeof task.task_id === 'string' ? task.task_id : null;
  if (!taskId) throw { code: 'unavailable', message: 'agent.task.enqueue returned no task_id' };
  const now = ctx.now();
  const existing = await ctx.get(TASKS, taskId);
  if (!existing) {
    await ctx.put(TASKS, {
      id: taskId,
      command_id: command.id,
      command_type: command.type,
      record_id: recordId,
      title,
      status: 'queued',
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
  return { ok: true, command_id: command.id, task_id: taskId, status: 'queued' };
}
