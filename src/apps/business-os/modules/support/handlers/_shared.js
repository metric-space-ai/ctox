// Origin: CTOX
// License: Apache-2.0
//
// Gemeinsame Fachlogik der support-Handler (Shell v4, Strang H3).
// Treuer Port von src/core/business_os/support.rs @ 4794af03d. Jede Funktion nennt ihre
// Rust-Quelle als `// ref: support.rs:<zeilen>`. Kein Dateisystem, kein Netz, keine
// Node-APIs: nur ECMAScript-Builtins (läuft im vm-Kontext der App-Laufzeit, §16.9).

// ---------------------------------------------------------------------------
// Fehler (Handler-Vertrag: throw {code, message})
// ---------------------------------------------------------------------------

export function fail(code, message) {
  throw { code, message };
}

// ---------------------------------------------------------------------------
// Rechte (nur Deklaration; durchgesetzt wird zentral im Core, §3 U5)
// ref: support.rs:392-421, policy.rs:478-490
// ---------------------------------------------------------------------------

export const SUPPORT_COMMAND_PERMISSIONS = Object.freeze({
  'support.inbox.upsert': 'support.manage_inboxes',
  'support.conversation.open_from_thread': 'support.triage',
  'support.conversation.claim': 'support.triage',
  'support.conversation.assign': 'support.assign',
  'support.conversation.status': 'support.triage',
  'support.conversation.priority': 'support.triage',
  'support.conversation.snooze': 'support.triage',
  'support.conversation.resolve': 'support.resolve',
  'support.conversation.reopen': 'support.triage',
  'support.identity.link': 'support.triage',
  'support.note.create': 'support.triage',
  'support.ticket.link': 'support.triage',
  'support.ticket.create_from_conversation': 'support.triage',
  'support.reply.draft': 'support.reply',
  'support.reply.send': 'support.reply',
  'support.view.upsert': 'support.triage',
  'support.view_filter.upsert': 'support.triage',
  'support.bulk.assign': 'support.assign',
  'support.bulk.status': 'support.triage',
  'support.bulk.priority': 'support.triage',
  'support.bulk.snooze': 'support.triage',
  'support.bulk.resolve': 'support.resolve',
  'support.assignment_policy.upsert': 'support.assign',
  'support.macro.upsert': 'support.manage_macros',
  'support.macro.run': 'support.triage',
  'support.automation_rule.upsert': 'support.manage_inboxes',
  'support.automation.evaluate': 'support.triage',
  'support.sla_policy.upsert': 'support.manage_sla',
  'support.sla.apply': 'support.manage_sla',
  'support.sla.recalculate': 'support.manage_sla',
  'support.reporting.rebuild_rollups': 'support.manage_inboxes',
  'support.agent.writeback': 'support.agent_request',
  'support.agent.apply_suggestion': 'support.agent_apply',
  'support.agent.reject_suggestion': 'support.agent_apply',
  // Strang SKL-A1: „CTOX fragen“ als App-Automatisierung; die Abschlussbefehle stellt der Core zu (§16.30).
  'support.agent.task': 'support.agent_request',
  'support.agent.task.complete': 'support.agent_request',
  'support.agent.task.failed': 'support.agent_request',
});

// ---------------------------------------------------------------------------
// Feldzugriff wie serde_json::Value::get + Hilfsfunktionen
// ---------------------------------------------------------------------------

function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function hasKey(value, key) {
  return isObject(value) && Object.prototype.hasOwnProperty.call(value, key);
}

// ref: support.rs:4213-4222
export function firstString(value, keys) {
  for (const key of keys) {
    if (!hasKey(value, key)) continue;
    const item = value[key];
    if (typeof item !== 'string') continue;
    const trimmed = item.trim();
    if (trimmed) return trimmed;
  }
  return undefined;
}

// ref: support.rs:4224-4232 – nur ganze Zahlen (as_i64/as_u64); 0.8 gilt als „fehlt“ (§3 U6).
export function numberField(value, keys) {
  for (const key of keys) {
    if (!hasKey(value, key)) continue;
    const item = value[key];
    if (typeof item === 'number' && Number.isInteger(item)) return item;
  }
  return undefined;
}

// ref: support.rs:4234-4248
export function boolField(value, keys) {
  for (const key of keys) {
    if (!hasKey(value, key)) continue;
    const item = value[key];
    if (typeof item === 'boolean') return item;
    if (typeof item === 'string') {
      const raw = item.trim().toLowerCase();
      if (['true', '1', 'yes', 'on'].includes(raw)) return true;
      if (['false', '0', 'no', 'off'].includes(raw)) return false;
    }
  }
  return undefined;
}

// ref: support.rs:4250-4253
export function jsonField(value, keys) {
  for (const key of keys) {
    if (!hasKey(value, key)) continue;
    const item = value[key];
    if (item !== null && item !== undefined) return clone(item);
  }
  return undefined;
}

// ref: support.rs:4255-4269 – erster VORHANDENER Schlüssel entscheidet, auch wenn er kein Array ist.
export function stringArrayField(value, keys) {
  for (const key of keys) {
    if (!hasKey(value, key)) continue;
    const item = value[key];
    if (!Array.isArray(item)) return [];
    return item
      .filter((entry) => typeof entry === 'string')
      .map((entry) => entry.trim())
      .filter(Boolean);
  }
  return [];
}

export function clone(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

function deepEqual(left, right) {
  if (left === right) return true;
  if (Array.isArray(left) && Array.isArray(right)) {
    return left.length === right.length && left.every((item, index) => deepEqual(item, right[index]));
  }
  if (isObject(left) && isObject(right)) {
    const lk = Object.keys(left);
    const rk = Object.keys(right);
    return lk.length === rk.length && lk.every((key) => hasKey(right, key) && deepEqual(left[key], right[key]));
  }
  return false;
}

function asciiLower(value) {
  return value.replace(/[A-Z]/g, (ch) => ch.toLowerCase());
}

// ref: support.rs:4283-4291
export function jsonScalarString(value) {
  if (value === null || value === undefined) return '';
  if (typeof value === 'string') return value;
  if (typeof value === 'boolean' || typeof value === 'number') return String(value);
  return JSON.stringify(value);
}

// ref: support.rs:4271-4281
export function jsonScalarEq(left, right) {
  const l = left === undefined ? null : left;
  const r = right === undefined ? null : right;
  if (deepEqual(l, r)) return true;
  if (typeof l === 'string' && typeof r === 'string') return asciiLower(l) === asciiLower(r);
  if (typeof l === 'string') return l === jsonScalarString(r);
  if (typeof r === 'string') return jsonScalarString(l) === r;
  return false;
}

// ref: support.rs:4293-4299
export function jsonContains(left, right) {
  if (Array.isArray(left)) return left.some((item) => jsonScalarEq(item, right));
  if (typeof left === 'string') return left.includes(jsonScalarString(right));
  return false;
}

// ref: support.rs:4301-4309
export function valueIsEmpty(value) {
  if (value === null || value === undefined) return true;
  if (typeof value === 'string') return value.trim() === '';
  if (Array.isArray(value)) return value.length === 0;
  if (isObject(value)) return Object.keys(value).length === 0;
  return false;
}

// ref: support.rs:4311-4318
export function normalizeStatus(status) {
  switch (String(status).trim().toLowerCase()) {
    case 'resolved': case 'closed': case 'done': return 'resolved';
    case 'snoozed': case 'sleeping': return 'snoozed';
    case 'waiting': case 'pending': return 'waiting';
    default: return 'open';
  }
}

// ref: support.rs:4320-4327
export function normalizePriority(priority) {
  switch (String(priority).trim().toLowerCase()) {
    case 'urgent': case 'critical': return 'urgent';
    case 'high': return 'high';
    case 'low': return 'low';
    default: return 'normal';
  }
}

// ref: support.rs:4329-4334
export function normalizeVisibility(visibility) {
  switch (String(visibility).trim().toLowerCase()) {
    case 'public': case 'customer': return 'public';
    default: return 'internal';
  }
}

// ref: support.rs:4336-4338 – zählt Unicode-Skalarwerte wie str::chars.
export function trimToChars(value, maxChars) {
  return Array.from(String(value)).slice(0, maxChars).join('');
}

// ref: support.rs:4347-4353
export function dayBucketStartMs(value) {
  return value <= 0 ? 0 : value - (value % 86_400_000);
}

// ---------------------------------------------------------------------------
// UUID v5 (SHA-1, RFC 4122) – reines JS, weil der vm-Kontext kein crypto hat.
// ---------------------------------------------------------------------------

export const NAMESPACE_URL = '6ba7b811-9dad-11d1-80b4-00c04fd430c8';

function utf8Bytes(text) {
  const out = [];
  for (const ch of String(text)) {
    const cp = ch.codePointAt(0);
    if (cp < 0x80) out.push(cp);
    else if (cp < 0x800) out.push(0xc0 | (cp >> 6), 0x80 | (cp & 63));
    else if (cp < 0x10000) out.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
    else out.push(0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 63), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
  }
  return out;
}

function sha1(bytes) {
  const msg = bytes.slice();
  const bitLen = msg.length * 8;
  msg.push(0x80);
  while (msg.length % 64 !== 56) msg.push(0);
  const hi = Math.floor(bitLen / 0x100000000);
  const lo = bitLen >>> 0;
  msg.push((hi >>> 24) & 255, (hi >>> 16) & 255, (hi >>> 8) & 255, hi & 255);
  msg.push((lo >>> 24) & 255, (lo >>> 16) & 255, (lo >>> 8) & 255, lo & 255);
  let h0 = 0x67452301; let h1 = 0xefcdab89; let h2 = 0x98badcfe; let h3 = 0x10325476; let h4 = 0xc3d2e1f0;
  const w = new Array(80);
  for (let offset = 0; offset < msg.length; offset += 64) {
    for (let i = 0; i < 16; i += 1) {
      const j = offset + i * 4;
      w[i] = ((msg[j] << 24) | (msg[j + 1] << 16) | (msg[j + 2] << 8) | msg[j + 3]) >>> 0;
    }
    for (let i = 16; i < 80; i += 1) {
      const x = w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16];
      w[i] = ((x << 1) | (x >>> 31)) >>> 0;
    }
    let a = h0; let b = h1; let c = h2; let d = h3; let e = h4;
    for (let i = 0; i < 80; i += 1) {
      let f; let k;
      if (i < 20) { f = (b & c) | (~b & d); k = 0x5a827999; }
      else if (i < 40) { f = b ^ c ^ d; k = 0x6ed9eba1; }
      else if (i < 60) { f = (b & c) | (b & d) | (c & d); k = 0x8f1bbcdc; }
      else { f = b ^ c ^ d; k = 0xca62c1d6; }
      const temp = ((((a << 5) | (a >>> 27)) >>> 0) + (f >>> 0) + e + k + w[i]) >>> 0;
      e = d; d = c; c = ((b << 30) | (b >>> 2)) >>> 0; b = a; a = temp;
    }
    h0 = (h0 + a) >>> 0; h1 = (h1 + b) >>> 0; h2 = (h2 + c) >>> 0; h3 = (h3 + d) >>> 0; h4 = (h4 + e) >>> 0;
  }
  const out = [];
  for (const h of [h0, h1, h2, h3, h4]) out.push((h >>> 24) & 255, (h >>> 16) & 255, (h >>> 8) & 255, h & 255);
  return out;
}

export function uuidV5(namespace, name) {
  const ns = namespace.replace(/-/g, '');
  const nsBytes = [];
  for (let i = 0; i < 32; i += 2) nsBytes.push(parseInt(ns.slice(i, i + 2), 16));
  const hash = sha1(nsBytes.concat(utf8Bytes(name))).slice(0, 16);
  hash[6] = (hash[6] & 0x0f) | 0x50;
  hash[8] = (hash[8] & 0x3f) | 0x80;
  const hex = hash.map((byte) => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

// ---------------------------------------------------------------------------
// Laufkontext je Befehl (Rust: handle_business_command Kopf, support.rs:423-432, 1324-1332)
// ---------------------------------------------------------------------------

function toMs(value) {
  if (typeof value === 'number') return Math.trunc(value);
  if (value instanceof Date) return value.getTime();
  return new Date(value).getTime();
}

export function payloadOf(command) {
  return isObject(command?.payload) ? command.payload : {};
}

// command.record_id ist NICHT Teil des Handler-Vertrags; wird gelesen, falls der Core ihn
// weiterreicht (Rust-Fallback). Siehe §3 U3.
export function recordIdOf(command) {
  return typeof command?.record_id === 'string' ? command.record_id : undefined;
}

export function createRun(command, ctx) {
  const commandId = typeof command?.id === 'string' ? command.id : '';
  if (!commandId) fail('invalid', 'command id is required');
  const actorId = typeof command?.actor?.id === 'string' && command.actor.id
    ? command.actor.id
    : 'business-os'; // ref: support.rs:4205-4211
  const run = {
    command,
    commandId,
    commandType: command.type,
    actorId,
    payload: payloadOf(command),
    now: toMs(ctx.now()),
    seq: 0,
    projections: [],
    // Rust: Uuid::new_v4(). Hier deterministisch aus Befehls-ID + Zähler (§3 D2).
    newId(prefix) {
      run.seq += 1;
      return `${prefix}${uuidV5(NAMESPACE_URL, `ctox.support:${commandId}:${run.seq}`)}`;
    },
    // ref: support.rs:4144-4162
    touch(collection, recordId) {
      if (!String(recordId || '').trim()) return;
      if (run.projections.some((p) => p.collection === collection && p.record_id === recordId)) return;
      run.projections.push({ collection, record_id: recordId });
    },
    result(fields) {
      return { ...fields, ok: true, command_type: run.commandType, projections: run.projections.map((p) => ({ ...p })) };
    },
  };
  return run;
}

// Ein Schreibvorgang je Befehl in einer Transaktion (Rust: Autocommit je Anweisung, §3 D1).
export async function withStore(ctx, run, fn) {
  const seqAtStart = run.seq;
  const projectionsAtStart = run.projections.length;
  return ctx.transaction(async (tx) => {
    run.seq = seqAtStart;
    run.projections.length = projectionsAtStart;
    return fn(new SupportStore(tx, run));
  });
}

export class SupportStore {
  constructor(tx, run) {
    this.tx = tx;
    this.run = run;
  }

  // Dokumente mit is_deleted === true gelten als nicht vorhanden (§3 D3).
  async get(collection, id) {
    const doc = await this.tx.get(collection, id);
    if (!doc || doc.is_deleted === true) return null;
    return doc;
  }

  async read(collection, query) {
    const docs = await this.tx.read(collection, query);
    return (docs || []).filter((doc) => doc && doc.is_deleted !== true);
  }

  async put(collection, doc) {
    await this.tx.put(collection, doc);
    this.run.touch(collection, doc.id);
    return doc;
  }
}

// ---------------------------------------------------------------------------
// Konversation
// ---------------------------------------------------------------------------

// ref: support.rs:27-48 + Tabellen-Defaults :53-74
export function conversationFromDoc(doc) {
  return {
    id: String(doc.id),
    inbox_id: doc.inbox_id ?? '',
    primary_thread_key: doc.primary_thread_key ?? '',
    status: doc.status ?? 'open',
    priority: doc.priority ?? 'normal',
    assignee_id: doc.assignee_id ?? '',
    team_id: doc.team_id ?? '',
    customer_account_id: doc.customer_account_id ?? '',
    customer_contact_id: doc.customer_contact_id ?? '',
    ticket_case_id: doc.ticket_case_id ?? '',
    last_message_key: doc.last_message_key ?? '',
    last_activity_at_ms: doc.last_activity_at_ms ?? 0,
    waiting_since_ms: doc.waiting_since_ms ?? 0,
    snoozed_until_ms: doc.snoozed_until_ms ?? 0,
    unread_count: doc.unread_count ?? 0,
    label_ids: doc.label_ids === undefined ? [] : clone(doc.label_ids),
    custom_attributes: doc.custom_attributes === undefined ? {} : clone(doc.custom_attributes),
    search_text: doc.search_text ?? '',
    created_at_ms: doc.created_at_ms ?? 0,
    updated_at_ms: doc.updated_at_ms ?? 0,
  };
}

// ref: support.rs:3510-3547 (project_conversation, Feldreihenfolge wie Rust)
export function conversationDoc(c) {
  return {
    id: c.id,
    is_deleted: false,
    created_at_ms: c.created_at_ms,
    updated_at_ms: c.updated_at_ms,
    inbox_id: c.inbox_id,
    primary_thread_key: c.primary_thread_key,
    status: c.status,
    priority: c.priority,
    assignee_id: c.assignee_id,
    team_id: c.team_id,
    customer_account_id: c.customer_account_id,
    customer_contact_id: c.customer_contact_id,
    ticket_case_id: c.ticket_case_id,
    last_message_key: c.last_message_key,
    last_activity_at_ms: c.last_activity_at_ms,
    waiting_since_ms: c.waiting_since_ms,
    snoozed_until_ms: c.snoozed_until_ms,
    unread_count: c.unread_count,
    label_ids: c.label_ids,
    custom_attributes: c.custom_attributes,
    search_text: c.search_text,
  };
}

// ref: support.rs:3023-3066
export async function loadConversation(store, conversationId) {
  const doc = await store.get('support_conversations', conversationId);
  return doc ? conversationFromDoc(doc) : null;
}

// ref: support.rs:3015-3021
export async function loadConversationRequired(store, conversationId) {
  const conversation = await loadConversation(store, conversationId);
  if (!conversation) fail('not_found', `support conversation \`${conversationId}\` does not exist`);
  return conversation;
}

async function saveConversation(store, conversation) {
  await store.put('support_conversations', conversationDoc(conversation));
  return conversation;
}

// ref: support.rs:1508-1626
export async function upsertConversationFromPayload(store, run, conversationId, payload) {
  const existing = await loadConversation(store, conversationId);
  const pick = (value, fallback, deflt) => (value !== undefined ? value : (existing ? existing[fallback] : deflt));
  const status = firstString(payload, ['status']);
  const priority = firstString(payload, ['priority']);
  const conversation = {
    id: conversationId,
    inbox_id: pick(firstString(payload, ['inbox_id']), 'inbox_id', ''),
    primary_thread_key: pick(firstString(payload, ['primary_thread_key', 'thread_key']), 'primary_thread_key', conversationId),
    status: pick(status !== undefined ? normalizeStatus(status) : undefined, 'status', 'open'),
    priority: pick(priority !== undefined ? normalizePriority(priority) : undefined, 'priority', 'normal'),
    assignee_id: pick(firstString(payload, ['assignee_id']), 'assignee_id', ''),
    team_id: pick(firstString(payload, ['team_id']), 'team_id', ''),
    customer_account_id: pick(firstString(payload, ['customer_account_id']), 'customer_account_id', ''),
    customer_contact_id: pick(firstString(payload, ['customer_contact_id']), 'customer_contact_id', ''),
    ticket_case_id: pick(firstString(payload, ['ticket_case_id', 'case_id', 'ticket_key']), 'ticket_case_id', ''),
    last_message_key: pick(firstString(payload, ['last_message_key', 'message_key']), 'last_message_key', ''),
    last_activity_at_ms: pick(numberField(payload, ['last_activity_at_ms', 'occurred_at_ms']), 'last_activity_at_ms', run.now),
    waiting_since_ms: pick(numberField(payload, ['waiting_since_ms']), 'waiting_since_ms', 0),
    snoozed_until_ms: pick(numberField(payload, ['snoozed_until_ms']), 'snoozed_until_ms', 0),
    unread_count: pick(numberField(payload, ['unread_count']), 'unread_count', 0),
    // Rust: payload.get("label_ids") – vorhandener Schlüssel gewinnt, auch mit null.
    label_ids: hasKey(payload, 'label_ids') ? clone(payload.label_ids) : (existing ? existing.label_ids : []),
    custom_attributes: hasKey(payload, 'custom_attributes')
      ? clone(payload.custom_attributes)
      : (existing ? existing.custom_attributes : {}),
    search_text: pick(firstString(payload, ['search_text', 'title', 'summary', 'subject']), 'search_text', conversationId),
    created_at_ms: existing ? existing.created_at_ms : run.now,
    updated_at_ms: run.now,
  };
  return saveConversation(store, conversation);
}

// ref: support.rs:1628-1638
export async function ensureConversation(store, run, conversationId, payload) {
  const existing = await loadConversation(store, conversationId);
  if (existing) return existing;
  return upsertConversationFromPayload(store, run, conversationId, payload);
}

// ref: support.rs:2904-2958 (Patch-Felder; undefined = unverändert)
const PATCH_FIELDS = [
  'status', 'priority', 'assignee_id', 'team_id', 'customer_account_id', 'customer_contact_id',
  'ticket_case_id', 'last_activity_at_ms', 'snoozed_until_ms',
];

export async function updateConversationFields(store, run, conversationId, patch) {
  const current = await loadConversationRequired(store, conversationId);
  for (const field of PATCH_FIELDS) {
    if (patch[field] !== undefined) current[field] = patch[field];
  }
  current.updated_at_ms = run.now;
  return saveConversation(store, current);
}

// ref: support.rs:2960-3009
export async function updateConversationStatus(store, run, conversationId, status) {
  const current = await loadConversationRequired(store, conversationId);
  const conversation = await updateConversationFields(store, run, conversationId, {
    status,
    snoozed_until_ms: status === 'open' ? 0 : current.snoozed_until_ms,
    last_activity_at_ms: run.now,
  });
  await insertConversationEvent(store, run, {
    conversation_id: conversationId,
    event_type: 'support.conversation.status_changed',
    summary: 'Support conversation status changed.',
    payload: { status },
  });
  await insertReportingEvent(store, run, conversationId, 'support.conversation.status_changed', 'count', 1, { status });
  return conversation;
}

// ref: support.rs:2045-2074 – bedingtes UPDATE; Atomik über ctx.transaction (§3 D1).
export async function claimConversationAtomic(store, run, conversationId, nextAssigneeId, force) {
  const current = await loadConversationRequired(store, conversationId);
  const previous = current.assignee_id;
  if (!(force || current.assignee_id === '' || current.assignee_id === nextAssigneeId)) {
    fail('conflict', `support conversation \`${conversationId}\` is already assigned to \`${current.assignee_id}\``);
  }
  current.assignee_id = nextAssigneeId;
  current.last_activity_at_ms = run.now;
  current.updated_at_ms = run.now;
  await saveConversation(store, current);
  return previous;
}

// ref: support.rs:2881-2890
async function loadInboxPolicy(store, inboxId) {
  const inbox = await store.get('support_inboxes', inboxId);
  if (!inbox) return null;
  return inbox.policy_json === undefined ? {} : inbox.policy_json;
}

// ref: support.rs:2076-2116
export async function ensureResolveRequirements(store, conversationId, payload) {
  const conversation = await loadConversationRequired(store, conversationId);
  const required = stringArrayField(payload, ['required_fields']);
  const custom = conversation.custom_attributes;
  if (isObject(custom) && Array.isArray(custom.resolve_required_fields)) {
    for (const field of custom.resolve_required_fields) if (typeof field === 'string') required.push(field);
  }
  if (conversation.inbox_id !== '') {
    const policy = await loadInboxPolicy(store, conversation.inbox_id);
    if (isObject(policy) && Array.isArray(policy.resolve_required_fields)) {
      for (const field of policy.resolve_required_fields) if (typeof field === 'string') required.push(field);
    }
  }
  const unique = [...new Set(required)].sort(byteCompare);
  const missing = unique.filter((field) => conversationRequiredFieldMissing(conversation, field));
  if (missing.length) {
    fail('conflict', `cannot resolve support conversation; missing required fields: ${missing.join(', ')}`);
  }
}

function byteCompare(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}

// ref: support.rs:2118-2137
function conversationRequiredFieldMissing(conversation, field) {
  switch (field) {
    case 'assignee_id':
    case 'customer_account_id':
    case 'customer_contact_id':
    case 'ticket_case_id':
    case 'primary_thread_key':
    case 'last_message_key':
      return conversation[field] === '';
    case 'label_ids':
      return Array.isArray(conversation.label_ids) ? conversation.label_ids.length === 0 : true;
    default: {
      const custom = conversation.custom_attributes;
      return hasKey(custom, field) ? valueIsEmpty(custom[field]) : true;
    }
  }
}

// ref: support.rs:4179-4185
export function commandConversationId(command) {
  const payload = payloadOf(command);
  const value = firstString(payload, ['conversation_id', 'id']) ?? recordIdOf(command);
  const trimmed = typeof value === 'string' ? value.trim() : '';
  if (!trimmed) fail('invalid', 'conversation_id is required');
  return trimmed;
}

// ref: support.rs:4187-4203
export function commandConversationIds(command) {
  const ids = stringArrayField(payloadOf(command), ['conversation_ids', 'ids']);
  if (!ids.length) {
    const recordId = (recordIdOf(command) || '').trim();
    if (recordId) ids.push(recordId);
  }
  const unique = [...new Set(ids)].sort(byteCompare);
  if (!unique.length) fail('invalid', 'conversation_ids is required');
  return unique;
}

// ---------------------------------------------------------------------------
// Ereignisse und Nebensätze
// ---------------------------------------------------------------------------

// ref: support.rs:3223-3254, 3682-3729
export async function insertConversationEvent(store, run, event) {
  const id = run.newId('support_event_');
  return store.put('support_conversation_events', {
    id,
    is_deleted: false,
    conversation_id: event.conversation_id,
    event_type: event.event_type,
    actor_id: event.actor_id ?? run.actorId,
    source_command_id: event.source_command_id ?? run.commandId,
    source_task_id: event.source_task_id ?? '',
    summary: event.summary ?? '',
    payload: event.payload === undefined ? {} : clone(event.payload),
    occurred_at_ms: run.now,
    created_at_ms: run.now,
    updated_at_ms: run.now,
  });
}

// ref: support.rs:3256-3283, 3731-3776
export async function insertAssignmentEvent(store, run, conversationId, assigneeId, previousAssigneeId, eventType, payload) {
  return store.put('support_assignment_events', {
    id: run.newId('support_assignment_'),
    is_deleted: false,
    conversation_id: conversationId,
    policy_id: '',
    assignee_id: assigneeId,
    previous_assignee_id: previousAssigneeId,
    event_type: eventType,
    occurred_at_ms: run.now,
    payload: clone(payload),
    created_at_ms: run.now,
    updated_at_ms: run.now,
  });
}

// ref: support.rs:3285-3310, 3968-3997
export async function insertSlaEvent(store, run, conversationId, appliedSlaId, eventType, payload) {
  return store.put('support_sla_events', {
    id: run.newId('support_sla_event_'),
    is_deleted: false,
    conversation_id: conversationId,
    applied_sla_id: appliedSlaId,
    event_type: eventType,
    occurred_at_ms: run.now,
    payload: clone(payload),
    created_at_ms: run.now,
    updated_at_ms: run.now,
  });
}

// ref: support.rs:3312-3339, 3999-4035
export async function insertReportingEvent(store, run, conversationId, eventName, metricName, valueMs, payload) {
  return store.put('support_reporting_events', {
    id: run.newId('support_reporting_event_'),
    is_deleted: false,
    conversation_id: conversationId,
    event_name: eventName,
    metric_name: metricName,
    value_ms: valueMs,
    occurred_at_ms: run.now,
    payload: clone(payload),
    created_at_ms: run.now,
    updated_at_ms: run.now,
  });
}

// ref: support.rs:3115-3137, 3644-3680 – bei Konflikt bleiben author_id, conversation_id, created_at_ms.
export async function insertNote(store, run, noteId, conversationId, authorId, body, visibility, source) {
  const existing = await store.get('support_notes', noteId);
  return store.put('support_notes', {
    id: noteId,
    is_deleted: false,
    conversation_id: existing ? existing.conversation_id : conversationId,
    author_id: existing ? existing.author_id : authorId,
    body,
    visibility,
    source,
    created_at_ms: existing ? existing.created_at_ms : run.now,
    updated_at_ms: run.now,
  });
}

// ref: support.rs:3139-3165, 3549-3591
export async function upsertThreadLink(store, run, conversationId, threadKey, channel, accountKey, linkRole) {
  const id = `support_thread_link_${uuidV5(NAMESPACE_URL, threadKey)}`;
  const existing = await store.get('support_thread_links', id);
  return store.put('support_thread_links', {
    id,
    is_deleted: false,
    conversation_id: conversationId,
    thread_key: threadKey,
    channel,
    account_key: accountKey,
    link_role: linkRole,
    created_at_ms: existing ? existing.created_at_ms : run.now,
    updated_at_ms: run.now,
  });
}

// ref: support.rs:3068-3081
export async function linkedConversationForThread(store, threadKey) {
  const links = await store.read('support_thread_links', { selector: { thread_key: threadKey, link_role: 'primary' } });
  return links.length ? links[0].conversation_id : null;
}

// ref: support.rs:3083-3113
export async function supportThreadLinkCurrent(store, conversationId, threadKey, channel, accountKey, linkRole) {
  const links = await store.read('support_thread_links', { selector: { thread_key: threadKey, link_role: linkRole } });
  const row = links[0];
  return Boolean(row && row.conversation_id === conversationId && row.channel === channel && row.account_key === accountKey);
}

// ref: support.rs:3167-3221, 3593-3642
export async function upsertIdentityLink(store, run, identityLinkId, payload) {
  const existing = await store.get('support_identity_links', identityLinkId);
  const externalIdentity = firstString(payload, ['external_identity', 'email', 'phone']) ?? '';
  return store.put('support_identity_links', {
    id: identityLinkId,
    is_deleted: false,
    channel: firstString(payload, ['channel', 'inbound_channel']) ?? '',
    account_key: firstString(payload, ['account_key']) ?? '',
    external_identity: externalIdentity,
    normalized_identity: firstString(payload, ['normalized_identity']) ?? asciiLower(externalIdentity.trim()),
    customer_account_id: firstString(payload, ['customer_account_id']) ?? '',
    customer_contact_id: firstString(payload, ['customer_contact_id']) ?? '',
    confidence: numberField(payload, ['confidence']) ?? 1,
    status: firstString(payload, ['status']) ?? 'active',
    source: firstString(payload, ['source']) ?? 'business-os.support',
    payload: clone(payload),
    created_at_ms: existing ? existing.created_at_ms : run.now,
    updated_at_ms: run.now,
  });
}

// ref: support.rs:3341-3387, 4076-4124
export async function upsertAgentSuggestion(store, run, s) {
  const existing = await store.get('support_agent_suggestions', s.id);
  return store.put('support_agent_suggestions', {
    id: s.id,
    is_deleted: false,
    conversation_id: s.conversation_id,
    source_command_id: s.source_command_id,
    task_id: s.task_id,
    suggestion_kind: s.suggestion_kind,
    status: s.status,
    confidence: s.confidence,
    required_human_action: s.required_human_action,
    summary: s.summary,
    payload: clone(s.payload),
    created_at_ms: existing ? existing.created_at_ms : run.now,
    updated_at_ms: run.now,
  });
}

// ref: support.rs:1963-1975
export function attachmentRefsFromPayload(payload) {
  return stringArrayField(payload, ['attachment_file_ids', 'file_ids']).map((fileId) => ({
    file_id: fileId,
    file_collection: 'desktop_files',
    chunk_collection: 'desktop_file_chunks',
  }));
}

// Konfig-Datensätze: created_at_ms bleibt beim Upsert erhalten (current_created_at, support.rs:2892-2902).
export async function upsertWithCreatedAt(store, run, collection, id, fields) {
  const existing = await store.get(collection, id);
  return store.put(collection, {
    id,
    is_deleted: false,
    ...fields,
    created_at_ms: existing ? (existing.created_at_ms ?? run.now) : run.now,
    updated_at_ms: run.now,
  });
}

// ---------------------------------------------------------------------------
// SLA
// ---------------------------------------------------------------------------

// ref: support.rs:2857-2863
function dueAt(startedAtMs, targetMs) {
  return targetMs <= 0 ? 0 : startedAtMs + targetMs;
}

// ref: support.rs:2865-2879
function slaIsBreached(now, first, next, resolution) {
  return [first, next, resolution].filter((due) => due > 0).some((due) => now > due);
}

// ref: support.rs:2541-2568
export async function resolveSlaPolicyForConversation(store, conversation) {
  if (conversation.inbox_id !== '') {
    const inbox = await store.get('support_inboxes', conversation.inbox_id);
    if (inbox && typeof inbox.sla_policy_id === 'string' && inbox.sla_policy_id !== '') return inbox.sla_policy_id;
  }
  const active = (await store.read('support_sla_policies', { selector: { active: true } }))
    .sort((a, b) => (b.updated_at_ms ?? 0) - (a.updated_at_ms ?? 0) || byteCompare(String(a.id), String(b.id)));
  return active.length ? active[0].id : null;
}

// ref: support.rs:2595-2696
export async function applySlaPolicy(store, run, conversation, policyId, startedAtMs) {
  const policy = await store.get('support_sla_policies', policyId);
  if (!policy) fail('not_found', `support SLA policy \`${policyId}\` does not exist`);
  if (policy.active === false) fail('conflict', `support SLA policy \`${policyId}\` is inactive`);
  const appliedSlaId = `support_applied_sla_${uuidV5(NAMESPACE_URL, `${conversation.id}:${policyId}`)}`;
  const existing = await store.get('support_applied_slas', appliedSlaId);
  // Rust: Option<i64> – vorhandene Zeile liefert ihren Wert, auch 0 (§3 U7).
  const existingBreachedAt = existing ? (existing.breached_at_ms ?? 0) : undefined;
  const first = dueAt(startedAtMs, policy.first_response_target_ms ?? 0);
  const next = dueAt(startedAtMs, policy.next_response_target_ms ?? 0);
  const resolution = dueAt(startedAtMs, policy.resolution_target_ms ?? 0);
  const breached = slaIsBreached(run.now, first, next, resolution);
  const breachedAtMs = breached ? (existingBreachedAt !== undefined ? existingBreachedAt : run.now) : 0;
  const status = conversation.status === 'resolved' ? 'resolved' : breached ? 'breached' : 'active';
  await store.put('support_applied_slas', {
    id: appliedSlaId,
    is_deleted: false,
    conversation_id: existing ? existing.conversation_id : conversation.id,
    policy_id: existing ? existing.policy_id : policyId,
    status,
    first_response_due_at_ms: first,
    next_response_due_at_ms: next,
    resolution_due_at_ms: resolution,
    breached_at_ms: breachedAtMs,
    payload: { started_at_ms: startedAtMs, source: 'support.sla.apply' },
    created_at_ms: existing ? existing.created_at_ms : run.now,
    updated_at_ms: run.now,
  });
  await insertSlaEvent(store, run, conversation.id, appliedSlaId, 'support.sla.applied', { policy_id: policyId });
  if (breached && existingBreachedAt === undefined) {
    await insertSlaEvent(store, run, conversation.id, appliedSlaId, 'support.sla.breached', {
      policy_id: policyId,
      first_response_due_at_ms: first,
      next_response_due_at_ms: next,
      resolution_due_at_ms: resolution,
    });
  }
  return appliedSlaId;
}

// ref: support.rs:2570-2593
export async function applySlaForConversationIfAvailable(store, run, conversation) {
  const policyId = await resolveSlaPolicyForConversation(store, conversation);
  if (!policyId) return null;
  const startedAtMs = conversation.waiting_since_ms > 0 ? conversation.waiting_since_ms : conversation.created_at_ms;
  return applySlaPolicy(store, run, conversation, policyId, startedAtMs);
}

// sla.apply-Pfad: payload.policy_id oder auflösen; Fehler beim Auflösen werden verschluckt (.ok().flatten()).
export async function applySlaFromPayload(store, run, conversationId, payload, missingMessage) {
  const conversation = await loadConversationRequired(store, conversationId);
  let policyId = firstString(payload, ['policy_id']);
  if (policyId === undefined) {
    try {
      policyId = (await resolveSlaPolicyForConversation(store, conversation)) ?? undefined;
    } catch {
      policyId = undefined;
    }
  }
  if (policyId === undefined) fail('invalid', missingMessage);
  const startedAtMs = numberField(payload, ['started_at_ms'])
    ?? (conversation.waiting_since_ms > 0 ? conversation.waiting_since_ms : conversation.created_at_ms);
  const appliedSlaId = await applySlaPolicy(store, run, conversation, policyId, startedAtMs);
  return { policyId, appliedSlaId };
}

// ref: support.rs:2698-2760
export async function recalculateSlas(store, run, conversationId) {
  const query = conversationId !== undefined ? { selector: { conversation_id: conversationId } } : { selector: {} };
  const rows = await store.read('support_applied_slas', query);
  let recalculated = 0;
  for (const row of rows) {
    const breachedAt = row.breached_at_ms ?? 0;
    const breached = slaIsBreached(run.now, row.first_response_due_at_ms ?? 0, row.next_response_due_at_ms ?? 0, row.resolution_due_at_ms ?? 0);
    const status = row.status === 'resolved' ? 'resolved' : breached ? 'breached' : 'active';
    const nextBreachedAt = breached && breachedAt === 0 ? run.now : breachedAt;
    await store.put('support_applied_slas', { ...clone(row), status, breached_at_ms: nextBreachedAt, updated_at_ms: run.now });
    if (breached && breachedAt === 0) {
      await insertSlaEvent(store, run, row.conversation_id, row.id, 'support.sla.breached', { source: 'support.sla.recalculate' });
    }
    recalculated += 1;
  }
  return recalculated;
}

// ref: support.rs:2762-2792
export async function markAppliedSlasResolved(store, run, conversationId) {
  const rows = await store.read('support_applied_slas', { selector: { conversation_id: conversationId } });
  for (const row of rows) {
    await store.put('support_applied_slas', { ...clone(row), status: 'resolved', updated_at_ms: run.now });
    await insertSlaEvent(store, run, conversationId, row.id, 'support.sla.resolved', { source: 'support.conversation.resolve' });
  }
}

// ---------------------------------------------------------------------------
// Geschlossene Aktionen (Makros, Automationen)  ref: support.rs:2139-2419
// ---------------------------------------------------------------------------

export async function applySupportActions(store, run, conversationId, actions, sourceLabel) {
  if (!Array.isArray(actions)) fail('invalid', 'support actions must be an array');
  const applied = [];
  for (const action of actions) {
    const actionType = firstString(action, ['type', 'command_type', 'action']);
    if (actionType === undefined) fail('invalid', 'support action type is required');
    const payload = hasKey(action, 'payload') ? action.payload : action;
    switch (actionType) {
      case 'support.note.create':
      case 'note.create': {
        const noteId = firstString(payload, ['id', 'note_id']) ?? run.newId('support_note_');
        const body = firstString(payload, ['body', 'text', 'note']);
        if (body === undefined) fail('invalid', 'macro note body is required');
        const visibility = normalizeVisibility(firstString(payload, ['visibility']) ?? 'internal');
        await insertNote(store, run, noteId, conversationId, run.actorId, body, visibility, sourceLabel);
        await insertConversationEvent(store, run, {
          conversation_id: conversationId,
          event_type: 'support.note.created',
          summary: 'Internal support note created by automation.',
          payload: { note_id: noteId, source: sourceLabel },
        });
        applied.push({ type: actionType, note_id: noteId });
        break;
      }
      case 'support.conversation.status':
      case 'status': {
        const raw = firstString(payload, ['status']);
        if (raw === undefined) fail('invalid', 'status action requires status');
        const status = normalizeStatus(raw);
        if (status === 'resolved') await ensureResolveRequirements(store, conversationId, payload);
        await updateConversationStatus(store, run, conversationId, status);
        if (status === 'resolved') await markAppliedSlasResolved(store, run, conversationId);
        applied.push({ type: actionType, status });
        break;
      }
      case 'support.conversation.priority':
      case 'priority': {
        const raw = firstString(payload, ['priority']);
        if (raw === undefined) fail('invalid', 'priority action requires priority');
        const priority = normalizePriority(raw);
        await updateConversationFields(store, run, conversationId, { priority, last_activity_at_ms: run.now });
        await insertConversationEvent(store, run, {
          conversation_id: conversationId,
          event_type: 'support.conversation.priority_changed',
          summary: 'Support conversation priority changed by automation.',
          payload: { priority, source: sourceLabel },
        });
        applied.push({ type: actionType, priority });
        break;
      }
      case 'support.conversation.assign':
      case 'assign': {
        const previous = await loadConversationRequired(store, conversationId);
        const assigneeId = firstString(payload, ['assignee_id', 'user_id']);
        const teamId = firstString(payload, ['team_id']);
        if (assigneeId === undefined && teamId === undefined) fail('invalid', 'assign action requires assignee_id or team_id');
        const conversation = await updateConversationFields(store, run, conversationId, {
          assignee_id: assigneeId,
          team_id: teamId,
          last_activity_at_ms: run.now,
        });
        await insertAssignmentEvent(store, run, conversationId, assigneeId ?? conversation.assignee_id, previous.assignee_id,
          'support.assignment.automated', { team_id: teamId ?? null, source_command_id: run.commandId });
        applied.push({ type: actionType, assignee_id: conversation.assignee_id, team_id: conversation.team_id });
        break;
      }
      case 'support.conversation.snooze':
      case 'snooze': {
        const until = numberField(payload, ['snoozed_until_ms', 'until_ms']);
        if (until === undefined) fail('invalid', 'snooze action requires snoozed_until_ms');
        await updateConversationFields(store, run, conversationId, { status: 'snoozed', snoozed_until_ms: until, last_activity_at_ms: run.now });
        await insertConversationEvent(store, run, {
          conversation_id: conversationId,
          event_type: 'support.conversation.snoozed',
          summary: 'Support conversation snoozed by automation.',
          payload: { snoozed_until_ms: until, source: sourceLabel },
        });
        applied.push({ type: actionType, snoozed_until_ms: until });
        break;
      }
      case 'support.ticket.link':
      case 'ticket.link': {
        const ticketCaseId = firstString(payload, ['ticket_case_id', 'case_id', 'ticket_key']);
        if (ticketCaseId === undefined) fail('invalid', 'ticket link action requires ticket_case_id');
        await updateConversationFields(store, run, conversationId, { ticket_case_id: ticketCaseId, last_activity_at_ms: run.now });
        await insertConversationEvent(store, run, {
          conversation_id: conversationId,
          event_type: 'support.ticket.linked',
          summary: 'Support conversation linked to a CTOX ticket by automation.',
          payload: { ticket_case_id: ticketCaseId, source: sourceLabel },
        });
        applied.push({ type: actionType, ticket_case_id: ticketCaseId });
        break;
      }
      case 'support.reply.draft':
      case 'reply.draft': {
        const body = firstString(payload, ['body', 'text', 'draft']);
        if (body === undefined) fail('invalid', 'reply draft action requires body');
        const suggestionId = firstString(payload, ['id', 'suggestion_id']) ?? run.newId('support_suggestion_');
        await upsertAgentSuggestion(store, run, {
          id: suggestionId,
          conversation_id: conversationId,
          source_command_id: run.commandId,
          task_id: '',
          suggestion_kind: 'draft_reply',
          status: 'draft',
          confidence: 1,
          required_human_action: 'human_send_required',
          summary: trimToChars(body, 180),
          payload: { body, source: sourceLabel },
        });
        applied.push({ type: actionType, suggestion_id: suggestionId });
        break;
      }
      case 'support.sla.apply':
      case 'sla.apply': {
        const { policyId, appliedSlaId } = await applySlaFromPayload(store, run, conversationId, payload, 'sla action requires policy_id');
        applied.push({ type: actionType, policy_id: policyId, applied_sla_id: appliedSlaId });
        break;
      }
      default:
        fail('invalid', `unsupported closed Support action \`${actionType}\``);
    }
  }
  return applied;
}

// ref: support.rs:2519-2539
function conversationFieldValue(conversation, field) {
  switch (field) {
    case 'id':
    case 'conversation_id':
      return conversation.id;
    case 'inbox_id': case 'status': case 'priority': case 'assignee_id': case 'team_id':
    case 'customer_account_id': case 'customer_contact_id': case 'ticket_case_id':
    case 'last_message_key': case 'primary_thread_key':
      return conversation[field];
    case 'label_ids':
      return conversation.label_ids;
    default:
      return hasKey(conversation.custom_attributes, field) ? conversation.custom_attributes[field] : null;
  }
}

// ref: support.rs:2498-2517
export function automationConditionMatch(conversation, condition) {
  const field = firstString(condition, ['field']);
  if (field === undefined) return false;
  const operator = firstString(condition, ['operator']) ?? 'eq';
  const actual = conversationFieldValue(conversation, field);
  const expected = hasKey(condition, 'value') ? condition.value : null;
  switch (operator) {
    case 'eq': case 'equals': return jsonScalarEq(actual, expected);
    case 'neq': case 'not_eq': case 'not_equals': return !jsonScalarEq(actual, expected);
    case 'in': return Array.isArray(expected) ? expected.some((item) => jsonScalarEq(actual, item)) : false;
    case 'contains': return jsonContains(actual, expected);
    case 'empty': return valueIsEmpty(actual);
    case 'not_empty': return !valueIsEmpty(actual);
    default: return false;
  }
}

// ref: support.rs:2477-2496
export function automationConditionsMatch(conversation, conditions, queryOperator) {
  if (!Array.isArray(conditions)) return false;
  if (!conditions.length) return true;
  const matches = conditions.map((condition) => automationConditionMatch(conversation, condition));
  return String(queryOperator).toLowerCase() === 'any' ? matches.some(Boolean) : matches.every(Boolean);
}

// ref: support.rs:2421-2475 – Bedingungen gegen den Stand VOR der ersten Regel (Schnappschuss).
export async function evaluateAutomationRules(store, run, conversationId, eventName, sourceLabel) {
  const conversation = await loadConversationRequired(store, conversationId);
  const rules = (await store.read('support_automation_rules', { selector: { event_name: eventName, active: true } }))
    .sort((a, b) => (a.updated_at_ms ?? 0) - (b.updated_at_ms ?? 0) || byteCompare(String(a.id), String(b.id)));
  const matched = [];
  for (const rule of rules) {
    const conditions = rule.conditions_json === undefined ? [] : rule.conditions_json;
    if (!automationConditionsMatch(conversation, conditions, rule.query_operator ?? 'all')) continue;
    const actions = rule.actions_json === undefined ? [] : rule.actions_json;
    const appliedActions = await applySupportActions(store, run, conversationId, actions, sourceLabel);
    run.touch('support_automation_rules', rule.id);
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.automation.rule_matched',
      summary: 'Support automation rule matched.',
      payload: { rule_id: rule.id, event_name: eventName, applied_actions: appliedActions },
    });
    matched.push({ rule_id: rule.id, applied_actions: appliedActions });
  }
  return matched;
}

// ---------------------------------------------------------------------------
// Reporting  ref: support.rs:1977-2043
// ---------------------------------------------------------------------------

export async function rebuildReportingRollups(store, run) {
  const events = (await store.read('support_reporting_events', { selector: {} }))
    .sort((a, b) => (a.occurred_at_ms ?? 0) - (b.occurred_at_ms ?? 0));
  const buckets = new Map();
  for (const event of events) {
    const eventName = event.event_name ?? '';
    const metricName = event.metric_name ?? 'count';
    const bucketStart = dayBucketStartMs(event.occurred_at_ms ?? 0);
    const key = JSON.stringify([eventName, metricName, bucketStart]);
    const entry = buckets.get(key) || { eventName, metricName, bucketStart, value: 0, count: 0 };
    entry.value += Number(event.value_ms ?? 0);
    entry.count += 1;
    buckets.set(key, entry);
  }
  for (const entry of buckets.values()) {
    const rollupKey = `support:${entry.eventName}:${entry.metricName}`;
    const id = `support_rollup_${uuidV5(NAMESPACE_URL, `${rollupKey}:${entry.bucketStart}`)}`;
    const existing = await store.get('support_reporting_rollups', id);
    await store.put('support_reporting_rollups', {
      id,
      is_deleted: false,
      rollup_key: rollupKey,
      bucket_start_ms: entry.bucketStart,
      bucket_end_ms: entry.bucketStart + 86_400_000,
      metric_name: entry.metricName,
      dimensions: { event_name: entry.eventName },
      value: entry.value,
      count: entry.count,
      created_at_ms: existing ? existing.created_at_ms : run.now,
      updated_at_ms: run.now,
    });
  }
  return buckets.size;
}
