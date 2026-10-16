// Shared helpers for the threads command handlers (Shell v4, Strang H2).
// Port of the helper layer of src/core/business_os/threads.rs @4794af03d
// (Z. 3500-4045 Upsert/Refresh, 4510-4885 Prüfungen, 4885-5245 Werkzeuge).
// Pure functions + ctx calls only: no filesystem, no network, no globals
// beyond ES builtins.

import { commandAccessCheck } from './_chat_access.js';

// ref: project_chats/supervisor_binding.rs:10
export const CONTRACT_SUPERVISOR_BINDING = 'ctox.workjet.supervisor_binding.v1';
/** Native Herkunft der Supervisor-Bindung, lesend als foreign_collection (Eigentümer ctox, V4-HANDLERS.md §6). */
export const SUPERVISOR_BINDINGS = 'workjet_supervisor_bindings';
const READ_PAGE = 200;

// ---- Fehler ---------------------------------------------------------------

export function fail(code, message) {
  // Handler contract: throw {code, message}.
  throw { code, message };
}

// ---- Werte (threads.rs:5011-5152) ----------------------------------------

export function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** serde_json `value.get(key).is_some()`: key present, explicit null counts. */
export function has(value, key) {
  return isObject(value) && Object.prototype.hasOwnProperty.call(value, key) && value[key] !== undefined;
}

/** `value.get(a).or_else(|| value.get(b))…` → first present value (null included). */
export function firstPresent(value, keys) {
  for (const key of keys) if (has(value, key)) return value[key];
  return undefined;
}

/** first_string_field: first key whose value is a non-empty string (trimmed). */
export function firstStringField(value, keys) {
  if (!isObject(value)) return undefined;
  for (const key of keys) {
    const v = value[key];
    if (typeof v === 'string' && v.trim() !== '') return v.trim();
  }
  return undefined;
}

export const nonEmptyString = (value, key) => firstStringField(value, [key]);
export const sourceString = (source, key) => firstStringField(source, [key]);

/** value_string: trimmed string or ''. */
export function valueString(value, key) {
  if (!isObject(value)) return '';
  const v = value[key];
  return typeof v === 'string' ? v.trim() : '';
}

export function nestedString(value, path) {
  let current = value;
  for (const key of path) {
    if (!isObject(current)) return undefined;
    current = current[key];
  }
  return typeof current === 'string' && current.trim() !== '' ? current.trim() : undefined;
}

/** required_string: anyhow "required text field is missing". */
export function requiredString(value, keys) {
  const found = firstStringField(value, keys);
  if (found === undefined) fail('invalid', 'required text field is missing');
  return found;
}

/** array_strings: string items only, trimmed, empties dropped (order kept). */
export function arrayStrings(value) {
  if (!Array.isArray(value)) return [];
  return value
    .filter((item) => typeof item === 'string')
    .map((item) => item.trim())
    .filter((item) => item !== '');
}

/** serde_json as_i64: only integral JSON numbers. */
export function asI64(value) {
  return typeof value === 'number' && Number.isSafeInteger(value) ? value : undefined;
}

export function asBool(value) {
  return typeof value === 'boolean' ? value : undefined;
}

export function firstNonEmpty(values) {
  for (const value of values) {
    const text = String(value ?? '').trim();
    if (text) return text;
  }
  return '';
}

/** Sorted, deduplicated copy (Rust Vec::sort + dedup / BTreeSet order). */
export function sortedUnique(values) {
  return [...new Set(values)].sort(compareRust);
}

/** Rust String ordering = byte order of UTF-8 = code point order. */
export function compareRust(a, b) {
  const ac = Array.from(a);
  const bc = Array.from(b);
  const n = Math.min(ac.length, bc.length);
  for (let i = 0; i < n; i += 1) {
    const d = ac[i].codePointAt(0) - bc[i].codePointAt(0);
    if (d !== 0) return d < 0 ? -1 : 1;
  }
  return ac.length === bc.length ? 0 : ac.length < bc.length ? -1 : 1;
}

/** truncate (threads.rs:5233): trim, then keep max_len chars (code points). */
export function truncate(value, maxLen) {
  const text = String(value ?? '').trim();
  const chars = Array.from(text);
  return chars.length <= maxLen ? text : chars.slice(0, maxLen).join('');
}

/** slug_part (threads.rs:5211). Every non [A-Za-z0-9_-] char becomes '_'. */
export function slugPart(value) {
  let slug = Array.from(String(value ?? ''))
    .map((ch) => (/^[A-Za-z0-9_-]$/.test(ch) ? ch : '_'))
    .join('');
  while (slug.includes('__')) slug = slug.replaceAll('__', '_');
  slug = slug.replace(/^_+|_+$/g, '');
  return slug === '' ? 'item' : truncate(slug, 80);
}

export function deepEqual(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b || a === null || b === null || typeof a !== 'object') return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) {
    if (a.length !== b.length) return false;
    return a.every((item, i) => deepEqual(item, b[i]));
  }
  const ak = Object.keys(a).filter((k) => a[k] !== undefined);
  const bk = Object.keys(b).filter((k) => b[k] !== undefined);
  if (ak.length !== bk.length) return false;
  return ak.every((k) => Object.prototype.hasOwnProperty.call(b, k) && deepEqual(a[k], b[k]));
}

export function clone(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

/** document_is_deleted: `_deleted` takes precedence over `is_deleted` when boolean. */
export function documentIsDeleted(value) {
  return firstPresent(value, ['_deleted', 'is_deleted']) === true;
}

/** document_updated_at_ms: first present key (not first integral). */
export function documentUpdatedAtMs(value) {
  return asI64(firstPresent(value, ['updated_at_ms', 'observed_at_ms', 'created_at_ms'])) ?? 0;
}

// ---- Akteur (threads.rs:5154-5201) ---------------------------------------
// U-1: Der v4-Befehl trägt actor:{id, role}. display_name und is_admin sind
// Vertragswunsch; ohne sie gilt Rust-Rückfall (display_name = id, Admin nur
// über Rolle chef|admin).

export function actorId(command) {
  const id = typeof command?.actor?.id === 'string' ? command.actor.id.trim() : '';
  return id || 'rxdb-command';
}

export function actorDisplayName(command) {
  const name = typeof command?.actor?.display_name === 'string' ? command.actor.display_name.trim() : '';
  return name || actorId(command);
}

export function actorRole(command) {
  const role = typeof command?.actor?.role === 'string' ? command.actor.role.trim() : '';
  return role || 'user';
}

export function isAdmin(command) {
  return command?.actor?.is_admin === true || ['chef', 'admin'].includes(actorRole(command));
}

export function actorPayload(command) {
  return {
    id: actorId(command),
    display_name: actorDisplayName(command),
    role: actorRole(command),
    is_admin: isAdmin(command),
  };
}

// ---- Befehlsfelder ---------------------------------------------------------

export function payloadOf(command) {
  return isObject(command?.payload) ? command.payload : {};
}

export function clientContextOf(command) {
  const cc = command?.client_context;
  return cc === undefined || cc === null ? null : cc;
}

/** command.record_id (Option<String>): undefined when absent, '' kept. */
export function commandRecordId(command) {
  const id = command?.record_id;
  return typeof id === 'string' ? id : undefined;
}

/** `first_string_field(payload, keys).or_else(|| command.record_id.clone()).context(msg)` */
export function idFromPayloadOrRecord(command, keys, message) {
  const id = firstStringField(payloadOf(command), keys) ?? commandRecordId(command);
  if (id === undefined) fail('invalid', message);
  return id;
}

export function commandModule(command) {
  return firstStringField(command, ['module', 'app_id']) ?? 'threads';
}

/** Rust `command.id.as_deref().context("command id is required")`. */
export function requiredCommandId(command) {
  if (typeof command?.id !== 'string') fail('invalid', 'command id is required');
  return command.id;
}

/** target_user_ids (threads.rs:4885). */
export function targetUserIds(payload) {
  const ids = arrayStrings(payload?.target_user_ids);
  const extra = firstStringField(payload, ['target_user_id', 'assignee_user_id']);
  if (extra !== undefined) ids.push(extra);
  return sortedUnique(ids);
}

/** source_context (threads.rs:4895). */
export function sourceContext(command) {
  const payload = payloadOf(command);
  const cc = isObject(command?.client_context) ? command.client_context : {};
  // Rust: `.get("source_context").or_else(|| .get("context")).filter(is_object)`:
  // a present source_context (even null) wins and is only then filtered.
  const raw = firstPresent(payload, ['source_context', 'context']);
  const source = isObject(raw) ? clone(raw) : {};
  ensureObjectField(source, 'module', () =>
    firstStringField(payload, ['source_module', 'module'])
      ?? firstStringField(cc, ['source_module', 'module'])
      ?? commandModule(command));
  ensureObjectField(source, 'record_type', () =>
    firstStringField(payload, ['record_type']) ?? firstStringField(cc, ['record_type']) ?? '');
  ensureObjectField(source, 'record_id', () =>
    firstStringField(payload, ['record_id'])
      ?? firstStringField(cc, ['record_id'])
      ?? commandRecordId(command)
      ?? '');
  ensureObjectField(source, 'label', () =>
    firstStringField(payload, ['label', 'source_label', 'title']) ?? firstStringField(cc, ['label']) ?? '');
  return source;
}

function ensureObjectField(object, key, fallback) {
  const existing = typeof object[key] === 'string' ? object[key].trim() : '';
  if (existing === '') object[key] = fallback();
}

/** thread_source_context (threads.rs:4927). */
export function threadSourceContext(record) {
  const meta = isObject(record?.metadata) ? record.metadata : null;
  if (meta && isObject(meta.source_context)) return clone(meta.source_context);
  return {
    module: valueString(record, 'source_module'),
    record_type: valueString(record, 'source_record_type'),
    record_id: valueString(record, 'source_record_id'),
    label: valueString(record, 'source_label'),
    deep_link: valueString(record, 'source_deep_link'),
  };
}

/** approval.source_context, else thread_source_context(approval). */
export function approvalSource(approval) {
  // Rust `approval.get("source_context").cloned()`: a present null stays null.
  return has(approval, 'source_context') ? clone(approval.source_context) : threadSourceContext(approval);
}

/** thread_id_for_command (threads.rs:4944). */
export function threadIdForCommand(ctx, command, source) {
  const explicit = firstStringField(payloadOf(command), ['thread_id']);
  if (explicit !== undefined) return explicit;
  const module = sourceString(source, 'module') ?? '';
  const recordType = sourceString(source, 'record_type') ?? '';
  const recordId = sourceString(source, 'record_id') ?? commandRecordId(command) ?? '';
  if (module !== '' && recordId !== '') {
    return truncate(`thread_${slugPart(module)}_${slugPart(recordType)}_${slugPart(recordId)}`, 220);
  }
  return `thread_${newUuid(ctx, command)}`;
}

export function threadStateId(userId, threadId) {
  return `thread_state_${slugPart(userId)}_${slugPart(threadId)}`;
}

// ---- IDs -------------------------------------------------------------------
// U-2: Rust nutzt Uuid::new_v4. Der Handler-Vertrag kennt keinen ID-Erzeuger;
// Wunsch ctx.uuid(). Rückfall: crypto.randomUUID, sonst deterministisch aus
// command.id + Zähler (stabil bei Wiederholung desselben Befehls).
const counters = new WeakMap();
export function newUuid(ctx, command) {
  if (typeof ctx?.uuid === 'function') return ctx.uuid();
  if (typeof globalThis.crypto?.randomUUID === 'function') return globalThis.crypto.randomUUID();
  const key = command || ctx;
  const n = (counters.get(key) || 0) + 1;
  counters.set(key, n);
  return `${slugPart(String(command?.id || 'cmd'))}-${n}`;
}

// ---- Projektionsliste (threads.rs:4967) -----------------------------------

export function projectionValues(projections) {
  const seen = new Set();
  const out = [];
  for (const p of projections) {
    const key = `${p.collection}/${p.record_id}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ collection: p.collection, record_id: p.record_id });
  }
  return out;
}

// ---- Speicherzugriff -------------------------------------------------------

/** load_record: empty id → null; RxDB-gelöscht → null; Soft-Delete (is_deleted) bleibt sichtbar. */
export async function loadRecord(store, collection, id) {
  if (typeof id !== 'string' || id.trim() === '') return null;
  const doc = await store.get(collection, id);
  if (!doc || doc._deleted === true) return null;
  return doc;
}

/** store::upsert_business_record: id + updated_at_ms gesetzt, Rest Core. */
export async function upsertRecord(store, collection, id, updatedAtMs, record) {
  const doc = clone(record);
  doc.id = id;
  doc.updated_at_ms = updatedAtMs;
  delete doc._rev;
  delete doc._meta;
  delete doc._deleted;
  await store.put(collection, doc);
}

/** upsert_projection_record_if_changed (threads.rs:4002). */
export async function upsertIfChanged(store, collection, id, updatedAtMs, record, projections) {
  const existing = await store.get(collection, id);
  if (existing && existing._deleted !== true) {
    const cmp = clone(existing);
    delete cmp._rev;
    delete cmp._meta;
    delete cmp._deleted;
    const next = clone(record);
    next.id = id;
    if (asI64(existing.updated_at_ms) === updatedAtMs && deepEqual(cmp, next)) return false;
  }
  await upsertRecord(store, collection, id, updatedAtMs, record);
  projections.push({ collection, record_id: id });
  return true;
}

/** thread_collection_records: alle nicht gelöschten Dokumente eines Threads, seitenweise. */
export async function threadCollectionRecords(store, collection, threadId) {
  const out = [];
  for (let skip = 0; ; skip += READ_PAGE) {
    const page = await store.read(collection, {
      selector: { thread_id: threadId },
      sort: [{ id: 'asc' }],
      skip,
      limit: READ_PAGE,
    });
    for (const doc of page) if (!documentIsDeleted(doc)) out.push(doc);
    if (page.length < READ_PAGE) break;
  }
  return out;
}

// ---- Capabilities (Wünsche, siehe V4-HANDLERS.md §3) -----------------------

function cap(ctx, path) {
  let node = ctx?.cap;
  for (const part of path) node = node?.[part];
  if (typeof node !== 'function') fail('unavailable', `capability ctx.cap.${path.join('.')} is not available`);
  return node;
}

/**
 * supervisor_binding::for_thread: Bindung (owner_user_id, thread_id) → Projekt, gelesen aus der
 * foreign_collection workjet_supervisor_bindings (id = project_id). Keine Zeile → null.
 * ref: project_chats/supervisor_binding.rs:133-154
 */
export async function supervisorBindingForThread(store, ownerUserId, threadId) {
  const owner = String(ownerUserId ?? '');
  const thread = String(threadId ?? '');
  if (thread === '') return null;
  const rows = await store.read(SUPERVISOR_BINDINGS, {
    selector: { owner_user_id: owner, thread_id: thread },
    sort: [{ id: 'asc' }],
    limit: 2,
  });
  const row = rows.find((doc) => !documentIsDeleted(doc));
  if (!row) return null;
  const projectId = typeof row.project_id === 'string' && row.project_id !== '' ? row.project_id : String(row.id ?? '');
  if (projectId === '') return null;
  return {
    contract: CONTRACT_SUPERVISOR_BINDING,
    project_id: projectId,
    thread_id: thread,
    thread_key: `business-os/threads/${thread}`,
  };
}

/** store::module_policy_decision → {allowed, display_reason} (CAP-3b: {allowed, display_reason, reason_code}; Akteur aus ctx). */
export async function moduleDecision(ctx, permission, module) {
  const decision = await cap(ctx, ['policy', 'module_decision'])({ permission, module });
  return { allowed: decision?.allowed === true, display_reason: String(decision?.display_reason ?? '') };
}

/** ensure_source_context_read_policy (threads.rs:4651). */
export async function ensureSourceContextReadPolicy(ctx, source) {
  const module = sourceString(source, 'module') ?? '';
  if (module === '' || module === 'threads') return;
  const decision = await moduleDecision(ctx, 'data.read', module);
  if (!decision.allowed) {
    fail('forbidden', `source context for module \`${module}\` is not readable by actor: ${decision.display_reason}`);
  }
}

/** active_business_user_display_name (threads.rs:4516). */
export async function activeBusinessUserDisplayName(ctx, userId) {
  const id = String(userId ?? '').trim();
  if (id === '') fail('invalid', 'reviewer_user_id is required');
  // CAP-3b #844 caps/directory.rs: directory.user({user_id}) → {found, user_id, display_name?, active}.
  const user = await cap(ctx, ['directory', 'user'])({ user_id: id });
  if (user?.found !== true) fail('invalid', `approval reviewer \`${id}\` is not an active Business OS user`);
  if (!user.active) fail('invalid', `approval reviewer \`${id}\` is inactive`);
  const name = String(user.display_name ?? '').trim();
  return name || id;
}

/** store::insert_business_event → Audit-Capability. */
export async function auditEvent(ctx, event) {
  await cap(ctx, ['audit', 'event'])(event);
}

// ---- Berechtigungen (threads.rs:4814-4883) ---------------------------------

export function ensureThreadParticipantOrAdmin(command, thread, message = 'only participants or admins can update this thread') {
  const actor = actorId(command);
  if (!(isAdmin(command) || arrayStrings(thread?.participant_ids).includes(actor))) fail('forbidden', message);
}

export async function ensureExistingThreadParticipantOrAdmin(store, command, threadId) {
  const thread = await loadRecord(store, 'user_threads', threadId);
  if (thread) ensureThreadParticipantOrAdmin(command, thread);
  return thread;
}

export function ensureMessageAuthorOrAdmin(command, message) {
  const author = valueString(message, 'author_user_id');
  if (!(isAdmin(command) || (author !== '' && author === actorId(command)))) {
    fail('forbidden', 'only message author or admin can edit this message');
  }
}

/** participant_set (threads.rs:4852): existing ∪ mandatory ∪ extra, sorted. */
export async function participantSet(store, threadId, mandatory, extra) {
  const existing = await loadRecord(store, 'user_threads', threadId);
  const set = new Set(arrayStrings(existing?.participant_ids));
  for (const id of [...mandatory, ...extra]) {
    const t = String(id ?? '').trim();
    if (t) set.add(t);
  }
  return [...set].sort(compareRust);
}

// ---- Upserts (threads.rs:3500-3763) ---------------------------------------

/**
 * upsert_thread. `binding` muss vorab per supervisorBindingForThread(owner)
 * gelesen werden (foreign_collection workjet_supervisor_bindings).
 */
export async function upsertThread(store, args, projections) {
  const { threadId, title, kind, status, participants, source, command, assignedUserId,
    lastMessageId, now, pendingDelta, binding } = args;
  const existing = (await loadRecord(store, 'user_threads', threadId)) || {};
  const createdAt = asI64(existing.created_at_ms) ?? now;
  const pending = Math.max(0, (asI64(existing.pending_approval_count) ?? 0) + pendingDelta);
  const actor = actorId(command);
  const owner = typeof existing.owner_user_id === 'string' && existing.owner_user_id.trim() !== ''
    ? existing.owner_user_id : actor;
  const createdBy = typeof existing.created_by_id === 'string' && existing.created_by_id.trim() !== ''
    ? existing.created_by_id : actor;
  const merged = new Set(arrayStrings(existing.participant_ids));
  for (const p of participants) merged.add(p);
  const metadata = { source_context: source };
  if (binding) {
    if (!(sourceString(source, 'module') === 'ctox'
      && sourceString(source, 'record_type') === 'workjet_project'
      && sourceString(source, 'record_id') === binding.project_id)) {
      fail('conflict', 'supervisor source context conflicts with its native project binding');
    }
    metadata.workjet_supervisor_contract = binding.contract || CONTRACT_SUPERVISOR_BINDING;
    metadata.workjet_supervisor = {
      project_id: binding.project_id,
      thread_id: binding.thread_id,
      thread_key: binding.thread_key,
    };
  }
  const record = {
    id: threadId,
    thread_id: threadId,
    title,
    kind,
    status,
    participant_ids: [...merged].sort(compareRust),
    watcher_user_ids: arrayStrings(existing.watcher_user_ids),
    owner_user_id: owner,
    created_by_id: createdBy,
    assigned_user_id: assignedUserId,
    source_module: sourceString(source, 'module') ?? '',
    source_record_type: sourceString(source, 'record_type') ?? '',
    source_record_id: sourceString(source, 'record_id') ?? '',
    source_label: sourceString(source, 'label') ?? '',
    source_deep_link: sourceString(source, 'deep_link') ?? '',
    last_message_id: lastMessageId ?? (typeof existing.last_message_id === 'string' ? existing.last_message_id : ''),
    last_message_at_ms: now,
    pending_approval_count: pending,
    snoozed_until_ms: asI64(existing.snoozed_until_ms) ?? 0,
    archived_at_ms: asI64(existing.archived_at_ms) ?? 0,
    last_seen_by_user: has(existing, 'last_seen_by_user') ? clone(existing.last_seen_by_user) : {},
    metadata,
    created_at_ms: createdAt,
    updated_at_ms: now,
  };
  await upsertIfChanged(store, 'user_threads', threadId, now, record, projections);
}

/** Owner whose supervisor binding upsert_thread will consult. */
export async function threadOwnerForBinding(store, command, threadId) {
  const existing = await loadRecord(store, 'user_threads', threadId);
  const owner = existing && typeof existing.owner_user_id === 'string' && existing.owner_user_id.trim() !== ''
    ? existing.owner_user_id : actorId(command);
  return owner;
}

/** upsert_thread_status_delta (threads.rs:3589). */
export async function upsertThreadStatusDelta(store, threadId, status, lastMessageId, now, pendingDelta, projections) {
  const existing = await loadRecord(store, 'user_threads', threadId);
  if (!existing) fail('not_found', `thread ${threadId} not found`);
  const next = clone(existing);
  next.status = status;
  if (lastMessageId !== undefined && lastMessageId !== null) {
    next.last_message_id = lastMessageId;
    next.last_message_at_ms = now;
  }
  next.pending_approval_count = Math.max(0, (asI64(existing.pending_approval_count) ?? 0) + pendingDelta);
  await upsertRecord(store, 'user_threads', threadId, now, next);
  projections.push({ collection: 'user_threads', record_id: threadId });
}

/** upsert_message (threads.rs:3622). */
export async function upsertMessage(store, args, projections) {
  const { threadId, messageId, kind, command, targetUserIds, body, source, approvalRequestId, commandId, now } = args;
  const record = {
    id: messageId,
    message_id: messageId,
    thread_id: threadId,
    kind,
    event_type: kind,
    actor_type: ['ctox_task', 'ctox_result', 'ai_result'].includes(kind) ? 'ai' : 'user',
    actor_id: actorId(command),
    parent_event_id: '',
    execution_status: kind === 'ai_request' ? 'requested' : 'completed',
    evidence_refs: [],
    author_user_id: actorId(command),
    author_display_name: actorDisplayName(command),
    target_user_ids: [...targetUserIds],
    body,
    source_module: sourceString(source, 'module') ?? '',
    source_record_type: sourceString(source, 'record_type') ?? '',
    source_record_id: sourceString(source, 'record_id') ?? '',
    source_label: sourceString(source, 'label') ?? '',
    source_deep_link: sourceString(source, 'deep_link') ?? '',
    approval_request_id: approvalRequestId,
    command_id: commandId,
    metadata: { source_context: source },
    created_at_ms: now,
    updated_at_ms: now,
  };
  await upsertIfChanged(store, 'user_thread_messages', messageId, now, record, projections);
}

/** upsert_source_link (threads.rs:3675). */
export async function upsertSourceLink(store, threadId, source, now, projections) {
  const module = sourceString(source, 'module') ?? '';
  const recordId = sourceString(source, 'record_id') ?? '';
  if (module === '' && recordId === '') return;
  const linkId = `link_${slugPart(threadId)}_${slugPart(module)}_${slugPart(recordId)}`;
  const record = {
    id: linkId,
    thread_id: threadId,
    source_module: module,
    source_record_type: sourceString(source, 'record_type') ?? '',
    source_record_id: recordId,
    source_label: sourceString(source, 'label') ?? '',
    link_role: 'source',
    deep_link: sourceString(source, 'deep_link') ?? '',
    context: source,
    created_at_ms: now,
    updated_at_ms: now,
  };
  await upsertIfChanged(store, 'user_thread_links', linkId, now, record, projections);
}

/** upsert_notifications (threads.rs:3718). */
export async function upsertNotifications(store, args, projections) {
  const { threadId, messageId, approvalRequestId, notificationType, userIds, title, body, source, now } = args;
  for (const userId of userIds) {
    if (!userId) continue;
    const notificationId = `notif_${slugPart(messageId)}_${slugPart(userId)}`;
    const record = {
      id: notificationId,
      notification_id: notificationId,
      user_id: userId,
      thread_id: threadId,
      message_id: messageId,
      approval_request_id: approvalRequestId,
      notification_type: notificationType,
      status: 'unread',
      title,
      body_preview: truncate(body, 180),
      source_module: sourceString(source, 'module') ?? '',
      source_record_id: sourceString(source, 'record_id') ?? '',
      created_at_ms: now,
      updated_at_ms: now,
    };
    await upsertIfChanged(store, 'user_notifications', notificationId, now, record, projections);
  }
}

const CLOSED = ['archived', 'completed', 'closed', 'cancelled', 'canceled'];

/** refresh_thread_states (threads.rs:3765): Aufmerksamkeit je relevantem Nutzer. */
export async function refreshThreadStates(store, threadId, now, projections) {
  const thread = await loadRecord(store, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  const relevant = new Set(arrayStrings(thread.participant_ids));
  for (const id of arrayStrings(thread.watcher_user_ids)) relevant.add(id);
  const assignedNonEmpty = nonEmptyString(thread, 'assigned_user_id');
  if (assignedNonEmpty !== undefined) relevant.add(assignedNonEmpty);

  const approvals = await threadCollectionRecords(store, 'ctox_task_approval_requests', threadId);
  const pendingReviewers = new Set();
  for (const approval of approvals) {
    if (valueString(approval, 'status') === 'pending') {
      const reviewer = nonEmptyString(approval, 'reviewer_user_id');
      if (reviewer !== undefined) {
        pendingReviewers.add(reviewer);
        relevant.add(reviewer);
      }
    }
  }

  const messages = await threadCollectionRecords(store, 'user_thread_messages', threadId);
  let latestHandoff = null;
  let latestCtoxStatus = null;
  for (const message of messages) {
    for (const id of arrayStrings(message.target_user_ids)) relevant.add(id);
    const kind = firstNonEmpty([valueString(message, 'kind'), valueString(message, 'event_type')]);
    if (kind === 'handoff' && (!latestHandoff || documentUpdatedAtMs(message) > documentUpdatedAtMs(latestHandoff))) {
      latestHandoff = message;
    }
    if (kind === 'ctox_status' && (!latestCtoxStatus || documentUpdatedAtMs(message) > documentUpdatedAtMs(latestCtoxStatus))) {
      latestCtoxStatus = message;
    }
  }

  const notifications = await threadCollectionRecords(store, 'user_notifications', threadId);
  const unreadByUser = new Map();
  const unreadMentions = new Set();
  for (const n of notifications) {
    if (valueString(n, 'status') !== 'unread') continue;
    const userId = valueString(n, 'user_id');
    if (userId === '') continue;
    unreadByUser.set(userId, (unreadByUser.get(userId) || 0) + 1);
    if (['mention', 'mentioned'].includes(valueString(n, 'notification_type'))) unreadMentions.add(userId);
  }

  const assigned = valueString(thread, 'assigned_user_id');
  const threadStatus = valueString(thread, 'status').toLowerCase();
  const machineWork = valueString(thread, 'kind') === 'ctox_task';
  if (threadStatus === 'needs_review') {
    for (const n of notifications) {
      if (['approval_request', 'approval_requested'].includes(valueString(n, 'notification_type'))) {
        const reviewer = nonEmptyString(n, 'user_id');
        if (reviewer !== undefined) {
          pendingReviewers.add(reviewer);
          relevant.add(reviewer);
        }
      }
    }
  }
  const handoffIsOpen = !CLOSED.includes(threadStatus);
  const openHandoffTargets = new Set();
  if (handoffIsOpen && assigned !== '' && latestHandoff
    && arrayStrings(latestHandoff.target_user_ids).includes(assigned)) {
    openHandoffTargets.add(assigned);
  }
  let latestCtoxFailed = false;
  if (latestCtoxStatus) {
    const mid = firstNonEmpty([valueString(latestCtoxStatus, 'message_id'), valueString(latestCtoxStatus, 'id')]);
    latestCtoxFailed = ['failed', 'error'].includes(valueString(latestCtoxStatus, 'execution_status').toLowerCase())
      || notifications.some((n) => valueString(n, 'message_id') === mid
        && ['ctox_failed', 'ai_failed'].includes(valueString(n, 'notification_type')));
  }
  const blocked = ['blocked', 'escalated'].includes(threadStatus);

  for (const userId of [...relevant].sort(compareRust)) {
    if (userId.trim() === '') continue;
    const unreadCount = unreadByUser.get(userId) || 0;
    const reasons = [];
    let score = 0;
    const actionable = !CLOSED.includes(threadStatus);
    if (pendingReviewers.has(userId)) { reasons.push('Freigabe nötig'); score = Math.max(score, 80); }
    if (unreadMentions.has(userId) && actionable) { reasons.push('Erwähnung'); score = Math.max(score, 60); }
    if (openHandoffTargets.has(userId)) { reasons.push('Übergabe an dich'); score = Math.max(score, 65); }
    if (blocked && assigned === userId) { reasons.push('Blockiert'); score = Math.max(score, 70); }
    if (latestCtoxFailed && assigned === userId) { reasons.push('Fehlgeschlagen'); score = Math.max(score, 70); }
    if (reasons.length === 0 && actionable && !machineWork) {
      if (assigned === userId) { reasons.push('Zugewiesen'); score = 50; } else if (unreadCount > 0) { reasons.push('Ungelesen'); score = 40; }
    }
    const stateId = threadStateId(userId, threadId);
    const existing = (await loadRecord(store, 'user_thread_states', stateId)) || {};
    const record = {
      id: stateId,
      thread_id: threadId,
      user_id: userId,
      attention_score: score,
      attention_reasons: reasons,
      unread_count: unreadCount,
      priority: nonEmptyString(existing, 'priority') ?? 'normal',
      pinned: asBool(existing.pinned) ?? false,
      snoozed_until_ms: asI64(existing.snoozed_until_ms) ?? 0,
      follow_up_at_ms: asI64(existing.follow_up_at_ms) ?? 0,
      last_seen_at_ms: asI64(existing.last_seen_at_ms) ?? 0,
      notification_preferences: has(existing, 'notification_preferences') ? clone(existing.notification_preferences) : {},
      is_deleted: false,
      created_at_ms: asI64(existing.created_at_ms) ?? now,
      updated_at_ms: now,
    };
    await upsertIfChanged(store, 'user_thread_states', stateId, now, record, projections);
  }
}

// ---- Freigaben (threads.rs:4510-4649) -------------------------------------

export function approvalIdFromCommand(command) {
  return idFromPayloadOrRecord(command, ['approval_request_id', 'id'], 'approval_request_id is required');
}

export async function loadApproval(store, approvalId) {
  const approval = await loadRecord(store, 'ctox_task_approval_requests', approvalId);
  if (!approval) fail('not_found', `approval request ${approvalId} not found`);
  return approval;
}

export function ensurePendingApproval(approval) {
  if (valueString(approval, 'status') !== 'pending') fail('conflict', 'approval request is not pending');
}

export function ensureApprovalVersion(command, approval) {
  const payload = payloadOf(command);
  // Rust: `.get("expected_updated_at_ms").or_else(|| .get("expected_version")).and_then(as_i64)`
  const expected = asI64(firstPresent(payload, ['expected_updated_at_ms', 'expected_version']));
  if (expected === undefined) fail('invalid', 'expected_updated_at_ms is required');
  const current = asI64(approval.updated_at_ms) ?? 0;
  if (expected !== current) fail('conflict', 'approval request changed; reload before deciding');
}

export function ensureApprovalTargetUnchanged(command, approval) {
  const payload = payloadOf(command);
  for (const key of ['target_command_type', 'target_module', 'target_record_id']) {
    const next = firstStringField(payload, [key]);
    if (next !== undefined && next !== valueString(approval, key)) {
      fail('invalid', 'approval target cannot be edited after request');
    }
  }
  if (payload.target_payload !== undefined) {
    const current = approval.target_payload === undefined ? null : approval.target_payload;
    if (!deepEqual(payload.target_payload, current)) {
      fail('invalid', 'approval target payload cannot be edited after request');
    }
  }
}

export function ensureReviewerOrAdmin(command, approval) {
  const reviewer = valueString(approval, 'reviewer_user_id');
  if (!(isAdmin(command) || (reviewer !== '' && reviewer === actorId(command)))) {
    fail('forbidden', 'Nur der eingetragene Reviewer oder ein Admin darf entscheiden.');
  }
}

export function ensureApprovalEditor(command, approval) {
  const requester = valueString(approval, 'requester_user_id');
  const reviewer = valueString(approval, 'reviewer_user_id');
  const actor = actorId(command);
  if (!(isAdmin(command) || (requester !== '' && requester === actor) || (reviewer !== '' && reviewer === actor))) {
    fail('forbidden', 'only requester, reviewer, or admin can edit this approval request');
  }
}

/** ensure_approval_target_policy (threads.rs:4612). */
export async function ensureApprovalTargetPolicy(ctx, approval) {
  const module = nonEmptyString(approval, 'target_module') ?? nonEmptyString(approval, 'source_module') ?? 'ctox';
  const commandType = valueString(approval, 'target_command_type');
  const tp = isObject(approval.target_payload) ? approval.target_payload : {};
  const rawTarget = firstPresent(tp, ['target', 'mode']);
  const target = typeof rawTarget === 'string' ? rawTarget.trim().toLowerCase() : '';
  let permission;
  if (commandType === 'ctox.business_os.app.modify' || target === 'app') permission = 'apps.modify';
  else if (['read', 'ask', 'question'].includes(target)) permission = 'data.read';
  else permission = 'data.write';
  const decision = await moduleDecision(ctx, permission, module);
  if (!decision.allowed) {
    fail('forbidden', `reviewer cannot approve target \`${commandType}\` for module \`${module}\`: ${decision.display_reason}`);
  }
}

/** record_approval_decision_event (threads.rs:3448). */
export async function recordApprovalDecisionEvent(ctx, command, approval, decision, approvedCommandId, approvedTaskId, decisionNote, observedAtMs) {
  const approvalId = valueString(approval, 'approval_request_id');
  if (approvalId === '') fail('invalid', 'approval_request_id is required');
  const payload = {
    event_type: 'business_os.external_approval.decided',
    approval_request_id: approvalId,
    thread_id: valueString(approval, 'thread_id'),
    decision,
    decision_note: decisionNote,
    decision_command_id: typeof command.id === 'string' ? command.id : null,
    decision_command_type: command.type,
    requester_user_id: valueString(approval, 'requester_user_id'),
    reviewer_user_id: valueString(approval, 'reviewer_user_id'),
    decision_by: actorPayload(command),
    source_module: valueString(approval, 'source_module'),
    source_record_type: valueString(approval, 'source_record_type'),
    source_record_id: valueString(approval, 'source_record_id'),
    source_label: valueString(approval, 'source_label'),
    source_deep_link: valueString(approval, 'source_deep_link'),
    source_context: approvalSource(approval),
    target_command_type: valueString(approval, 'target_command_type'),
    target_module: valueString(approval, 'target_module'),
    target_record_id: valueString(approval, 'target_record_id'),
    prompt: valueString(approval, 'prompt'),
    approved_command_id: approvedCommandId,
    approved_task_id: approvedTaskId,
    observed_at_ms: observedAtMs,
  };
  await auditEvent(ctx, {
    collection: 'ctox_task_approval_requests',
    record_id: approvalId,
    event_type: 'business_os.external_approval.decided',
    payload,
    observed_at_ms: observedAtMs,
  });
}

export function approvalTitle(approval, prompt) {
  const label = valueString(approval, 'source_label');
  return label !== '' ? `Freigabe: ${label}` : truncate(prompt, 80);
}

/** Gemeinsamer Vorspann jedes threads-Befehls (handle_business_command threads.rs:1090). */
export async function preflight(ctx, command) {
  // project_chats::command_access_check (privacy.rs:473) – jetzt threads-Fachlogik in _chat_access.js.
  await commandAccessCheck(ctx, command, actorId(command));
  const module = firstStringField(command, ['module']) ?? firstStringField(command, ['app_id']);
  if (module !== 'threads') fail('invalid', 'threads commands require module=threads');
}
