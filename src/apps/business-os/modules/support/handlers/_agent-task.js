// Origin: CTOX
// License: Apache-2.0
//
// „CTOX fragen“ der Support-App als App-Automatisierung (Strang SKL-A1, Kern §16.30/§16.34).
// Vorher: die UI schickte den rohen Kernbefehl business_os.chat.task mit dem Phantom-Skill
// business-os-support-workflow und einem writeback_contract; der Agent sollte selbst support.agent.writeback
// zurückschreiben (support.rs:1229-1278). Jetzt:
//   support.agent.task           Template prompts/agent-task.md füllen → agent.task.enqueue mit Skill
//                                support/support-workflow, Auftrag als support_agent_requests-Zeile (queued)
//   support.agent.task.complete  Ergebnis des Skills auswerten → support_agent_suggestions (proposed) + Ereignis
//                                support.agent.writeback je Vorschlag (wie support.agent.writeback), Auftrag completed
//   support.agent.task.failed    Auftrag failed bzw. cancelled (Abbruch kommt als on_failed mit status 'cancelled')
// Beide Abschlussbefehle sind idempotent: ein Auftrag, der nicht mehr offen ist, liefert status 'ignored' ohne
// Schreibzugriff. Nur ECMAScript (läuft im vm-Kontext der App-Laufzeit, §16.9).

import { renderPrompt } from 'ctox:shared/prompt-template.js';
import {
  clone, commandConversationId, createRun, fail, firstString, insertConversationEvent, loadConversationRequired,
  NAMESPACE_URL, upsertAgentSuggestion, uuidV5, withStore,
} from './_shared.js';

export const AGENT_TASK_SKILL = 'support/support-workflow';
export const AGENT_TASK_COMPLETE = 'support.agent.task.complete';
export const AGENT_TASK_FAILED = 'support.agent.task.failed';
// ref: support-commands.mjs SUPPORT_AGENT_SUGGESTION_KINDS (UI-Seite; Handler importieren nur ./_*)
export const SUGGESTION_KINDS = Object.freeze(['summary', 'draft_reply', 'classification', 'next_action', 'customer_update', 'ticket_action']);
const REQUESTS = 'support_agent_requests';
const OPEN = new Set(['queued', 'running']);
const SNAPSHOT_MAX_CHARS = 6000; // Anweisung bleibt deutlich unter der 8 000-Zeichen-Kappung des Queue-Speichers
const MAX_SUGGESTIONS = 20;

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

// ---------------------------------------------------------------------------------------------- Schnappschuss

function shrinkStrings(value, max) {
  if (typeof value === 'string') return value.length > max ? `${value.slice(0, max)}…` : value;
  if (Array.isArray(value)) return value.map((v) => shrinkStrings(v, max));
  if (isObject(value)) return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, shrinkStrings(v, max)]));
  return value;
}

/** Schnappschuss als JSON-Text ≤ maxChars: älteste Listeneinträge zuerst weg, dann lange Texte kürzen. */
export function compactSnapshot(snapshot, maxChars = SNAPSHOT_MAX_CHARS) {
  let snap = isObject(snapshot) ? clone(snapshot) : {};
  const fits = () => JSON.stringify(snap).length <= maxChars;
  // aufsteigend sortierte Listen (älteste vorn) bzw. absteigend (neueste vorn)
  const oldestFirst = ['recent_messages', 'events', 'notes', 'thread_links'];
  const newestFirst = ['suggestions'];
  while (!fits()) {
    const key = [...oldestFirst, ...newestFirst].find((k) => Array.isArray(snap[k]) && snap[k].length > 0);
    if (!key) break;
    if (oldestFirst.includes(key)) snap[key].shift(); else snap[key].pop();
  }
  for (const max of [800, 300, 120]) if (!fits()) snap = shrinkStrings(snap, max);
  if (!fits()) snap = { conversation: shrinkStrings(isObject(snap.conversation) ? snap.conversation : {}, 80) };
  const text = JSON.stringify(snap);
  return text.length <= maxChars ? text : '{}';
}

// ---------------------------------------------------------------------------------------------- Auftrag

export async function enqueueAgentTask(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  const requestKind = firstString(payload, ['request_kind']) ?? 'summary';
  const snapshot = isObject(payload.record_snapshot) ? payload.record_snapshot : {};
  const conversation = await loadConversationRequired({ get: async (c, id) => { const d = await ctx.get(c, id); return d && d.is_deleted !== true ? d : null; } }, conversationId);
  const status = firstString(payload, ['status']) ?? (conversation.status || 'open');
  const priority = firstString(payload, ['priority']) ?? (conversation.priority || 'normal');
  const instruction = (await renderPrompt(ctx, 'agent-task', {
    conversation_id: conversationId, status, priority, snapshot: compactSnapshot(snapshot),
  })).replace(/\n$/, '');
  const title = firstString(payload, ['title']) ?? `Support CTOX task ${conversationId}`;
  const requestId = `support_agent_request_${uuidV5(NAMESPACE_URL, `ctox.support.agent:${run.commandId}`)}`;
  const writebackContract = {
    collection: 'support_agent_suggestions', record_id: conversationId, source_collection: 'support_conversations',
    allowed_suggestion_kinds: [...SUGGESTION_KINDS], required_human_action: 'review',
    on_complete: AGENT_TASK_COMPLETE, on_failed: AGENT_TASK_FAILED,
  };

  // 1. Auftrag festhalten (vor dem Enqueue, damit ein früher Abschlussbefehl die Zeile findet).
  const existing = await withStore(ctx, run, async (store) => {
    const prev = await store.get(REQUESTS, requestId);
    if (prev) return prev;
    await store.put(REQUESTS, {
      id: requestId, is_deleted: false, conversation_id: conversationId, request_kind: requestKind, status: 'queued',
      command_id: run.commandId, task_id: '', required_skills: [AGENT_TASK_SKILL], writeback_contract: writebackContract,
      payload: { title, status, priority }, created_at_ms: run.now, updated_at_ms: run.now,
    });
    return null;
  });
  if (existing && existing.task_id) {
    return run.result({ conversation_id: conversationId, request_id: requestId, task_id: existing.task_id, status: existing.status });
  }

  // 2. Agentenauftrag (Capability außerhalb der Transaktion).
  let task;
  try {
    task = await ctx.cap.agent.task.enqueue({
      title,
      instruction,
      skill: 'support/support-workflow',
      thread_key: `business-os/support/${conversationId}`,
      payload: { request_id: requestId, conversation_id: conversationId, request_kind: requestKind, source_command_id: run.commandId, record_snapshot: clone(snapshot), allowed_suggestion_kinds: [...SUGGESTION_KINDS] },
      idempotency_key: `support-agent:${run.commandId}`,
      on_complete: 'support.agent.task.complete',
      on_failed: 'support.agent.task.failed',
    });
  } catch (error) {
    await ctx.patch(REQUESTS, requestId, { status: 'failed', payload: { title, status, priority, error: errorText(error) }, updated_at_ms: run.now });
    throw error;
  }
  const taskId = isObject(task) && typeof task.task_id === 'string' ? task.task_id : '';
  if (!taskId) fail('unavailable', 'agent.task.enqueue returned no task_id');
  // 3. task_id an den Auftrag (ein Abschluss, der schon da war, hat die Zeile bereits gesetzt → nicht überschreiben).
  const now = await ctx.get(REQUESTS, requestId);
  if (now && !now.task_id) await ctx.patch(REQUESTS, requestId, { task_id: taskId, updated_at_ms: run.now });
  return run.result({ conversation_id: conversationId, request_id: requestId, task_id: taskId, status: now?.status ?? 'queued' });
}

// ---------------------------------------------------------------------------------------------- Abschluss

function requiredTaskId(payload) {
  const taskId = firstString(payload, ['task_id']);
  if (!taskId) fail('invalid', 'task_id is required');
  return taskId;
}

export function errorText(raw) {
  const text = typeof raw === 'string' ? raw
    : isObject(raw) && typeof raw.message === 'string' ? raw.message
      : raw === undefined || raw === null ? '' : JSON.stringify(raw);
  return text.replace(/\s+/g, ' ').trim().slice(0, 1000);
}

// Letztes JSON-Objekt in einem Antworttext: ```-Blöcke (letzter zuerst), dann ab jeder `{` von hinten.
function lastJsonObject(text) {
  const fenced = [...text.matchAll(/```(?:json)?\s*([\s\S]*?)```/g)].map((m) => m[1]).reverse();
  const tails = [];
  for (let i = text.lastIndexOf('{'); i >= 0; i = i > 0 ? text.lastIndexOf('{', i - 1) : -1) tails.push(text.slice(i));
  for (const raw of [...fenced, ...tails]) {
    const trimmed = raw.trim();
    const end = trimmed.lastIndexOf('}');
    if (!trimmed.startsWith('{') || end < 0) continue;
    try {
      const value = JSON.parse(trimmed.slice(0, end + 1));
      if (isObject(value)) return value;
    } catch { /* nächster Kandidat */ }
  }
  return null;
}

function normalizeSuggestion(raw) {
  if (!isObject(raw)) return null;
  const kind = firstString(raw, ['suggestion_kind', 'kind']);
  if (!kind || !SUGGESTION_KINDS.includes(kind)) return null;
  const confidence = typeof raw.confidence === 'number' && Number.isFinite(raw.confidence) ? Math.min(1, Math.max(0, raw.confidence)) : 0;
  const summary = (firstString(raw, ['summary', 'title']) ?? '').slice(0, 500);
  const payload = isObject(raw.payload) ? clone(raw.payload)
    : typeof raw.payload === 'string' ? { text: raw.payload }
      : typeof raw.text === 'string' ? { text: raw.text } : {};
  return { suggestion_kind: kind, summary, confidence, required_human_action: firstString(raw, ['required_human_action']) ?? 'review', payload };
}

/**
 * Antwortauswertung (skills/support-workflow/SKILL.md, Abschnitt Ergebnis): {suggestions:[…]} als Objekt oder als
 * letzter JSON-Block im Antworttext ({text|output|final_text} oder Text). {error} ohne suggestions = Fehlschlag.
 * Antworttext ohne JSON → ein Vorschlag summary mit dem Text (der Agent hat geantwortet, nur nicht strukturiert).
 */
export function parseAgentResult(result) {
  let value = result;
  let text = typeof result === 'string' ? result : null;
  if (isObject(value) && !Array.isArray(value.suggestions) && value.error === undefined) {
    text = [value.text, value.output, value.final_text].find((x) => typeof x === 'string') ?? null;
    value = null;
  }
  if (value === null || typeof value === 'string') value = text ? lastJsonObject(text) : null;
  if (isObject(value) && !Array.isArray(value.suggestions) && typeof value.error === 'string' && value.error.trim()) {
    return { error: value.error.trim(), suggestions: [], dropped: 0 };
  }
  if (isObject(value) && Array.isArray(value.suggestions)) {
    const list = value.suggestions.slice(0, MAX_SUGGESTIONS).map(normalizeSuggestion);
    return { error: null, suggestions: list.filter(Boolean), dropped: list.filter((s) => !s).length + Math.max(0, value.suggestions.length - MAX_SUGGESTIONS) };
  }
  const plain = (text ?? '').trim();
  if (plain) return { error: null, suggestions: [{ suggestion_kind: 'summary', summary: plain.split('\n')[0].slice(0, 500), confidence: 0, required_human_action: 'review', payload: { text: plain.slice(0, 20000) } }], dropped: 0 };
  return { error: null, suggestions: [], dropped: 0 };
}

// Auftrag zur task_id: über die Zeile (task_id) oder – falls der Abschluss vor dem Setzen der task_id kommt – über
// die Enqueue-Payload (agent.task.get → payload.request_id).
async function requestIdForTask(ctx, taskId) {
  const rows = ((await ctx.read(REQUESTS, { selector: { task_id: taskId } })) || []).filter((r) => r && r.is_deleted !== true);
  if (rows.length) return rows[0].id;
  const task = typeof ctx.cap?.agent?.task?.get === 'function' ? await ctx.cap.agent.task.get({ task_id: taskId }) : null;
  const requestId = isObject(task?.payload) ? firstString(task.payload, ['request_id']) : undefined;
  if (!requestId) fail('not_found', `no support agent request for task ${taskId}`);
  return requestId;
}

async function settle(command, ctx, taskId, fn) {
  const run = createRun(command, ctx);
  const requestId = await requestIdForTask(ctx, taskId);
  return withStore(ctx, run, async (store) => {
    const request = await store.get(REQUESTS, requestId);
    if (!request) fail('not_found', `no support agent request for task ${taskId}`);
    if (!OPEN.has(request.status)) {
      return { ok: true, status: 'ignored', command_type: run.commandType, request_id: requestId, task_id: taskId, projections: [] };
    }
    return fn(store, run, request);
  });
}

export async function completeAgentTask(command, ctx) {
  const payload = createRun(command, ctx).payload;
  const taskId = requiredTaskId(payload);
  const parsed = parseAgentResult(payload.result);
  if (parsed.error) return failRequest(command, ctx, taskId, 'failed', parsed.error);
  return settle(command, ctx, taskId, async (store, run, request) => {
    const suggestionIds = [];
    for (const [index, s] of parsed.suggestions.entries()) {
      const id = `support_suggestion_${uuidV5(NAMESPACE_URL, `ctox.support.agent:${taskId}:${index}`)}`;
      // ref: support.rs:1229-1278 (support.agent.writeback) – gleiche Zeile und gleiches Ereignis je Vorschlag
      await upsertAgentSuggestion(store, run, {
        id, conversation_id: request.conversation_id, source_command_id: request.command_id ?? '', task_id: taskId,
        suggestion_kind: s.suggestion_kind, status: 'proposed', confidence: s.confidence,
        required_human_action: s.required_human_action, summary: s.summary, payload: s.payload,
      });
      await insertConversationEvent(store, run, {
        conversation_id: request.conversation_id, event_type: 'support.agent.writeback', source_task_id: taskId,
        summary: 'CTOX Agent wrote a structured Support suggestion.',
        payload: { suggestion_id: id, suggestion_kind: s.suggestion_kind },
      });
      suggestionIds.push(id);
    }
    await store.put(REQUESTS, {
      ...request, status: 'completed', task_id: taskId,
      payload: { ...(isObject(request.payload) ? request.payload : {}), suggestion_ids: suggestionIds, dropped_suggestions: parsed.dropped },
      updated_at_ms: run.now,
    });
    return run.result({ conversation_id: request.conversation_id, request_id: request.id, task_id: taskId, status: 'completed', suggestion_ids: suggestionIds });
  });
}

function failRequest(command, ctx, taskId, status, error) {
  return settle(command, ctx, taskId, async (store, run, request) => {
    await store.put(REQUESTS, {
      ...request, status, task_id: taskId,
      payload: { ...(isObject(request.payload) ? request.payload : {}), error },
      updated_at_ms: run.now,
    });
    return run.result({ conversation_id: request.conversation_id, request_id: request.id, task_id: taskId, status, error });
  });
}

export async function failAgentTask(command, ctx) {
  const payload = createRun(command, ctx).payload;
  const taskId = requiredTaskId(payload);
  const cancelled = payload.status === 'cancelled';
  const error = errorText(payload.error) || (cancelled ? 'support agent task cancelled' : 'support agent task failed');
  return failRequest(command, ctx, taskId, cancelled ? 'cancelled' : 'failed', error);
}
