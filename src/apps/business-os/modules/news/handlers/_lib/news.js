// Gemeinsame Fachlogik der News-App (Port aus store_news_commands.rs, Stand 4794af03d).
import { fillTemplate, loadPromptTemplate, templateFields } from 'ctox:shared/prompt-template.js';

// Jede Funktion nennt ihr Rust-Vorbild. Kein Dateisystem, kein Netz.

export const NEWS_MODULE = 'news';
export const NEWS_ISSUE_GENERATE = 'news.issue.generate';
export const NEWS_TOPIC_SCHEDULE = 'news.topic.schedule';
export const NEWS_SOURCE_ADAPTER = 'news.source.adapter';
export const NEWS_SOURCE_AUTH_ASSIST = 'news.source.auth_assist';
export const NEWS_ISSUE_CONTRACT = 'ctox.news.issue.v1';
export const NEWS_SOURCE_CONTRACT = 'ctox.news.source.v1';
/** Skill im App-Paket (skills/news-aggregation/SKILL.md), Namensraum der App (§16.30/§16.34). */
export const NEWS_APP_SKILL = 'news/news-aggregation';
/** Generischer System-Skill des Core (src/skills/system/communication/universal-scraping), bleibt im Core. */
export const SCRAPE_SKILL = 'universal-scraping';
export const NEWS_ISSUE_COMPLETE = 'news.issue.complete';
export const NEWS_ISSUE_FAILED = 'news.issue.failed';
export const NEWS_SOURCE_ADAPTER_COMPLETE = 'news.source.adapter.complete';
export const NEWS_SOURCE_ADAPTER_FAILED = 'news.source.adapter.failed';
export const SCHEDULE_PREFIX = 'news:';
export const ROUTE_MARKER = 'ctox-news-schedule:';
export const MAX_POSTS = 40;
export const MAX_TEXT = 8_000;
export const CATEGORIES = ['model', 'tool', 'research', 'debate'];

export function fail(code, message) {
  throw { code, message };
}

const chars = (text) => Array.from(String(text));
const str = (v) => (typeof v === 'string' ? v : undefined);
const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

// ref: store_news_commands.rs valid_record_id :56 (Rust zaehlt Bytes, nur ASCII erlaubt -> gleich)
export function validRecordId(value) {
  return typeof value === 'string' && value.length > 0 && value.length <= 220 && /^[A-Za-z0-9_.:-]+$/.test(value);
}

export const scheduleName = (topicId) => `${SCHEDULE_PREFIX}${topicId}`;
export const topicThreadKey = (topicId) => `business-os/news/${topicId}`;

// ref: store_news_commands.rs account_auth_source :107
export function accountAuthSource(kind) {
  if (kind === 'x') return 'x.com';
  if (kind === 'reddit') return 'reddit.com';
  return undefined;
}

// ref: store.rs first_string_field :26959 – erstes nach trim nicht-leeres String-Feld, getrimmt.
export function firstStringField(payload, keys) {
  for (const key of keys) {
    const v = payload?.[key];
    if (typeof v === 'string' && v.trim()) return v.trim();
  }
  return undefined;
}

// Handler-Vertrag kennt kein record_id; falls die Laufzeit es mitliefert, gilt es als Rueckfall wie in Rust.
export const recordIdOf = (command) => str(command?.record_id);

// ref: store_news_commands.rs session_user_id :115
export function sessionUserId(command) {
  const id = str(command?.actor?.id)?.trim();
  if (!id) fail('forbidden', 'news command requires an authenticated user');
  return id;
}

// ref: store_news_commands.rs patch_record :84 – Felder einmischen + updated_at_ms;
// fehlt der Datensatz, passiert nichts (Ok(None)).
export async function patchRecord(ctx, collection, id, now, fields) {
  const existing = await ctx.get(collection, id);
  if (!existing) return null;
  const next = { ...existing, ...fields, updated_at_ms: now };
  await ctx.put(collection, next);
  return next;
}

// ref: store_news_commands.rs clip :810
export function clip(value, max) {
  return typeof value === 'string' ? chars(value.trim()).slice(0, max).join('') : '';
}

// ref: :820
export function httpUrl(value) {
  const url = clip(value, 2_000);
  return url.startsWith('https://') || url.startsWith('http://') ? url : undefined;
}

// Rust f64::round = kaufmaennisch weg von null.
const roundAway = (n) => (n < 0 ? -Math.round(-n) : Math.round(n));

// ref: :825
export function nonNegative(value, max) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 0;
  return roundAway(Math.min(Math.max(value, 0), max));
}

// ref: :833
export function publishedAtMs(post) {
  if (Number.isInteger(post?.published_at_ms)) return Math.max(post.published_at_ms, 0);
  const raw = str(post?.published_at);
  // RFC 3339 mit Zeitzone wie chrono::DateTime::parse_from_rfc3339
  if (raw && /^\d{4}-\d{2}-\d{2}[Tt ]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})$/.test(raw)) {
    const ms = Date.parse(raw.replace(' ', 'T'));
    if (Number.isFinite(ms)) return ms;
  }
  return 0;
}

// ref: store.rs extract_json_object :23799
export function extractJsonObject(text) {
  const trimmed = String(text ?? '').trim();
  try {
    const v = JSON.parse(trimmed);
    if (isObject(v)) return v;
  } catch {
    // weiter mit Klammersuche
  }
  const start = trimmed.indexOf('{');
  if (start < 0) return undefined;
  let depth = 0;
  let inString = false;
  let escaped = false;
  let end;
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
        end = i + 1;
        break;
      }
    }
  }
  if (end === undefined) return undefined;
  try {
    const v = JSON.parse(trimmed.slice(start, end));
    return isObject(v) ? v : undefined;
  } catch {
    return undefined;
  }
}

// ref: :919 normalize_post
export function normalizePost(raw) {
  const intro = clip(raw?.intro, 600);
  if (!intro) return undefined;
  const sources = (Array.isArray(raw?.sources) ? raw.sources : [])
    .slice(0, 12)
    .map((source) => {
      const url = httpUrl(source?.url);
      return url === undefined ? undefined : { label: clip(source?.label, 120), url, kind: clip(source?.kind, 40) };
    })
    .filter(Boolean);
  if (!sources.length) return undefined;
  let category = clip(raw?.category, 20);
  if (!CATEGORIES.includes(category)) category = 'other';
  let authorKind = clip(raw?.author_kind, 20);
  if (!['lab', 'community', 'media', 'person'].includes(authorKind)) authorKind = 'media';
  const post = {
    category,
    author_name: clip(raw?.author_name, 120),
    author_handle: clip(raw?.author_handle, 80).replace(/^@+/, ''),
    author_kind: authorKind,
    intro,
    body: clip(raw?.body, MAX_TEXT),
    hype_score: nonNegative(raw?.hype_score, 100),
    discussion_count: nonNegative(raw?.discussion_count, 1e9),
    published_at_ms: publishedAtMs(raw),
    sources,
  };
  const card = raw?.link_card;
  const cardUrl = card === undefined || card === null ? undefined : httpUrl(card.url);
  post.link_card = cardUrl === undefined ? {} : { url: cardUrl, title: clip(card.title, 200), image_url: httpUrl(card.image_url) ?? '' };
  const quote = raw?.quote;
  const quoteText = quote === undefined || quote === null ? '' : clip(quote.text, 600);
  post.quote = quoteText
    ? { text: quoteText, source: clip(quote.source, 120), meta: clip(quote.meta, 120), url: httpUrl(quote.url) ?? '' }
    : {};
  return post;
}

// ref: :844 parse_issue_reply
export function parseIssueReply(replyText) {
  const reply = String(replyText ?? '').trim();
  if (!reply) fail('invalid', 'news issue reply was empty');
  const value = extractJsonObject(reply);
  if (!value) fail('invalid', 'news issue reply did not contain a JSON object');
  const schema = str(value.schema);
  if (schema !== undefined && schema !== NEWS_ISSUE_CONTRACT) {
    fail('invalid', `news issue reply uses schema \`${schema}\`, expected ${NEWS_ISSUE_CONTRACT}`);
  }
  if (!Array.isArray(value.posts)) fail('invalid', 'news issue reply has no posts array');
  const posts = [];
  let dropped = 0;
  for (const raw of value.posts.slice(0, MAX_POSTS)) {
    const post = normalizePost(raw);
    if (post) posts.push(post);
    else dropped += 1;
  }
  dropped += Math.max(0, value.posts.length - MAX_POSTS);
  if (!posts.length) fail('invalid', 'news issue reply has no post with intro and a linked source');
  const trends = (Array.isArray(value.trends) ? value.trends : [])
    .slice(0, 10)
    .map((trend) => {
      const title = clip(trend?.title, 120);
      if (!title) return undefined;
      const idx = trend?.post_index;
      return {
        title,
        category: clip(trend?.category, 40),
        discussion_count: nonNegative(trend?.discussion_count, 1e9),
        post_index: Number.isInteger(idx) && idx >= 0 ? idx : null,
      };
    })
    .filter(Boolean);
  const sourceReport = (Array.isArray(value.source_report) ? value.source_report : [])
    .slice(0, 200)
    .map((item) => {
      const sourceId = clip(item?.source_id, 220);
      const status = clip(item?.status, 20);
      return validRecordId(sourceId) && ['ok', 'needs_auth', 'failed'].includes(status)
        ? [sourceId, status, clip(item?.note, 300)]
        : undefined;
    })
    .filter(Boolean);
  return { posts, trends, sourceReport, summary: clip(value.summary, 1_000), dropped };
}

// ref: :769 parse_source_reply
export function parseSourceReply(replyText) {
  const value = extractJsonObject(replyText);
  if (!value || value.schema !== NEWS_SOURCE_CONTRACT) return { status: 'ok', account: '', note: '' };
  let status = 'failed';
  if (value.status === 'ok' || value.status === 'connected') status = 'ok';
  else if (value.status === 'needs_auth' || value.status === 'needs_owner') status = 'needs_auth';
  const account = clip(value.account, 120);
  return {
    status,
    account: /\s/.test(account) ? '' : account,
    note: status === 'ok' ? '' : clip(value.note, 500),
  };
}

// ref: :446 generate_payload – Payload der geplanten Ausgabe. Schluessel wie in Rust, ohne `instruction`/
// `required_skills`: die Anweisung fuellt news.issue.generate aus prompts/issue.md (SKL-B1), `--task-id` = Befehls-ID.
export function generatePayload(topic, sources, issueId, issueDate) {
  const field = (key) => str(topic?.[key]) ?? '';
  const topicId = field('id');
  // Rust: json!({"id": source.get("id"), …}) -> fehlende Felder werden null.
  const opt = (v) => (v === undefined ? null : v);
  return {
    topic_id: topicId,
    issue_id: issueId,
    issue_date: issueDate,
    thread_key: topicThreadKey(topicId),
    output_contract: NEWS_ISSUE_CONTRACT,
    record_snapshot: {
      topic: {
        id: topicId,
        title: field('title'),
        prompt: field('prompt'),
        cadence: field('cadence'),
        language: field('language'),
        categories: topic?.categories ?? [],
        max_posts: topic?.max_posts ?? 12,
      },
      sources: sources.map((s) => ({
        id: opt(s.id),
        kind: opt(s.kind),
        label: opt(s.label),
        url: opt(s.url),
        adapter_kind: opt(s.adapter_kind),
        target_key: opt(s.target_key),
        auth_mode: opt(s.auth_mode),
        auth_source: accountAuthSource(str(s.kind)) ?? '',
        items: opt(s.items),
        status: opt(s.status),
      })),
    },
  };
}

// ---------------------------------------------------------------- Agenten-Start (SKL-B1, Kern §16.30/§16.34)

const LANGUAGE_LABEL = (language) => (language === 'en' ? 'Englisch' : 'Deutsch');

/**
 * Template aus prompts/<name>.md laden und streng fuellen (ctox:shared/prompt-template.js). Eine Zeile entfaellt,
 * wenn sie einen der `optional` genannten Platzhalter enthaelt und dessen Wert '' ist – das bildet
 * `lines.filter(Boolean)` des Bestands nach (core/news-model.mjs buildIssueInstruction). Fehlende Werte bleiben Fehler.
 */
export async function renderPromptLines(ctx, name, values, optional = []) {
  const template = await loadPromptTemplate(ctx, name);
  const kept = template.split('\n').filter((line) => !templateFields(line).some((f) => optional.includes(f) && values[f] === ''));
  return fillTemplate(kept.join('\n'), values).trim();
}

const commandRecordId = (command) => str(command?.record_id) ?? str(command?.payload?.record_id);

// Titel wie store.rs command_title :26970 (Anzeigename des Typs liegt ausserhalb; hier der rohe Typ).
function taskTitle(command) {
  const payload = command.payload ?? {};
  const title = typeof payload.title === 'string' ? payload.title.trim() : '';
  return title || `${command.type} · ${commandRecordId(command)?.trim() || 'new task'}`;
}

// Ausgangsbefehl reist im Task-Payload; die Rueckruf-Handler lesen ihn per agent.task.get.
const originOf = (command) => ({ id: command.id, type: command.type, record_id: commandRecordId(command) ?? null, payload: command.payload ?? {} });

// Ersetzt den Anhang „Payload-Vorschau“, den der Core bisher an jede Anweisung haengte (store.rs command_prompt).
const contextJson = (value) => JSON.stringify(value, null, 2);

/**
 * Werte fuer prompts/issue.md. Bestand: core/news-model.mjs buildIssueInstruction (UI) und
 * store_news_commands.rs generate_payload :446 (Planer) – vereinheitlicht auf die UI-Fassung.
 */
export function issuePromptValues(command, previousIssue = null) {
  const payload = command.payload ?? {};
  const snapshot = isObject(payload.record_snapshot) ? payload.record_snapshot : {};
  const topic = isObject(snapshot.topic) ? snapshot.topic : {};
  const sources = (Array.isArray(snapshot.sources) ? snapshot.sources : []).filter(isObject);
  const signedIn = sources.some((s) => str(s.auth_source) || accountAuthSource(str(s.kind)));
  return {
    topic_title: str(topic.title) ?? '',
    focus: str(topic.prompt) ?? '',
    language: LANGUAGE_LABEL(topic.language),
    max_posts: Number.isInteger(topic.max_posts) && topic.max_posts >= 0 ? topic.max_posts : 12,
    categories: (Array.isArray(topic.categories) ? topic.categories : []).join(', '),
    source_count: sources.length,
    source_line: sources.map((s) => `${str(s.kind) ?? 'web'}:${str(s.label) ?? ''}`).join('; ') || 'keine konfiguriert, nutze die Websuche',
    signed_in_task_id: signedIn ? command.id : '',
    previous_issue_date: str(previousIssue?.issue_date) ?? '',
    context_json: contextJson({
      command_id: command.id,
      topic_id: str(payload.topic_id) ?? '',
      issue_id: str(payload.issue_id) ?? '',
      issue_date: str(payload.issue_date) ?? '',
      previous_issue_id: str(payload.previous_issue_id) ?? '',
      output_contract: NEWS_ISSUE_CONTRACT,
      record_snapshot: snapshot,
    }),
  };
}
export const ISSUE_OPTIONAL_FIELDS = ['focus', 'signed_in_task_id', 'previous_issue_date'];

/** news.issue.generate (Knopf „Ausgabe jetzt erzeugen“ bzw. Planer): Template fuellen, Agenten starten. */
export async function enqueueIssue(ctx, command) {
  const payload = command.payload ?? {};
  const topicId = firstStringField(payload, ['topic_id']) ?? recordIdOf(command) ?? '';
  const issueId = firstStringField(payload, ['issue_id']) ?? '';
  if (!validRecordId(topicId)) fail('invalid', 'news issue command is missing topic_id');
  if (!validRecordId(issueId)) fail('invalid', 'news issue command is missing issue_id');
  const previousId = firstStringField(payload, ['previous_issue_id']);
  const previous = previousId && validRecordId(previousId) ? await ctx.get('news_issues', previousId) : null;
  const instruction = await renderPromptLines(ctx, 'issue', issuePromptValues(command, previous), ISSUE_OPTIONAL_FIELDS);
  const task = await ctx.cap.agent.task.enqueue({
    title: taskTitle(command),
    instruction,
    skill: NEWS_APP_SKILL,
    thread_key: str(payload.thread_key) ?? topicThreadKey(topicId),
    payload: { command: originOf(command), output_contract: NEWS_ISSUE_CONTRACT, source: NEWS_MODULE },
    idempotency_key: command.id,
    on_complete: NEWS_ISSUE_COMPLETE,
    on_failed: NEWS_ISSUE_FAILED,
  });
  const taskId = task?.task_id ?? null;
  if (taskId) await patchRecord(ctx, 'news_issues', issueId, ctx.now(), { task_id: taskId, command_id: command.id });
  return { task_id: taskId };
}

/** news.source.adapter (Zauberstab): Scraper einrichten (System-Skill) bzw. CTOX-Konto verbinden (App-Skill). */
export async function enqueueSourceAdapter(ctx, command) {
  const payload = command.payload ?? {};
  const sourceId = firstStringField(payload, ['source_id']) ?? recordIdOf(command) ?? '';
  if (!validRecordId(sourceId)) fail('invalid', 'news.source.adapter is missing source_id');
  const snapshot = isObject(payload.record_snapshot?.source) ? payload.record_snapshot.source : {};
  const topicId = firstStringField(payload, ['topic_id']) ?? str(snapshot.topic_id) ?? '';
  const authSource = str(payload.auth_source) || str(snapshot.auth_source) || accountAuthSource(str(snapshot.kind)) || '';
  const label = str(snapshot.label) ?? '';
  const context = contextJson({ command_id: command.id, topic_id: topicId, source_id: sourceId, auth_source: authSource, record_snapshot: payload.record_snapshot ?? {} });
  const items = Array.isArray(snapshot.items) ? snapshot.items.join(', ') : '';
  const instruction = authSource
    ? await renderPromptLines(ctx, 'source-connect', { source_label: label, auth_source: authSource, source_id: sourceId, command_id: command.id, context_json: context })
    : await renderPromptLines(ctx, 'source-adapter', {
      source_label: label,
      source_target: str(snapshot.url) || items || str(snapshot.kind) || '',
      target_key: str(snapshot.target_key) ?? '',
      context_json: context,
    });
  const task = await ctx.cap.agent.task.enqueue({
    title: taskTitle(command),
    instruction,
    skill: authSource ? NEWS_APP_SKILL : SCRAPE_SKILL,
    thread_key: str(payload.thread_key) ?? `business-os/news/${topicId}/sources`,
    payload: { command: originOf(command), output_contract: authSource ? NEWS_SOURCE_CONTRACT : null, source: NEWS_MODULE },
    idempotency_key: command.id,
    on_complete: NEWS_SOURCE_ADAPTER_COMPLETE,
    on_failed: NEWS_SOURCE_ADAPTER_FAILED,
  });
  return { task_id: task?.task_id ?? null };
}

// ---------------------------------------------------------------- Rueckruf (Kern §16.30 Nr. 3)

// #778 (main e5f057ead, store.rs refresh_business_command_queue_task_projection): endet der Lauf als
// failed | blocked | cancelled, gilt der Datensatz als fehlgeschlagen – Grund = Notiz/Fehler getrimmt,
// sonst der Standardgrund.
export const HOLD_STATUSES = ['failed', 'blocked', 'cancelled'];
export const HOLD_DEFAULT_REASON = 'CTOX hat die Ausgabe nicht abgeschlossen.';

export function holdReason(payload) {
  const error = payload?.error;
  const note = [
    payload?.status_note,
    typeof error === 'string' ? error : error?.message,
    payload?.result?.status_note,
    payload?.result?.error,
    payload?.result?.message,
  ].find((v) => typeof v === 'string' && v.trim() !== '');
  return note ? note.trim() : HOLD_DEFAULT_REASON;
}

/** Rueckruf {task_id, status, result, error?} → Ausgangsbefehl aus der eigenen Aufgabe (agent.task.get). */
export async function callbackOrigin(command, ctx, expectedType) {
  const taskId = firstStringField(command.payload ?? {}, ['task_id']);
  if (!taskId) fail('invalid', `${command.type} is missing task_id`);
  const task = await ctx.cap.agent.task.get({ task_id: taskId });
  const original = task?.payload?.command;
  if (!isObject(original) || original.type !== expectedType) fail('not_found', `news task ${taskId} carries no ${expectedType} command`);
  return { taskId, original, status: typeof command.payload?.status === 'string' ? command.payload.status : '' };
}

export function resultText(result) {
  if (typeof result === 'string') return result;
  if (typeof result?.text === 'string') return result.text;
  return JSON.stringify(result ?? '');
}

export const clipMessage = (message) => Array.from(String(message ?? '')).slice(0, 500).join('');
