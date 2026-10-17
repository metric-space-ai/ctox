// Gemeinsame Fachlogik der customers-Handler (Shell v4, Strang H4).
// Treue Portierung der Helfer aus store_customer_commands.rs und store.rs (outbound_*) @ 4794af03d.
// Kein Dateisystem, kein Netz, keine Node-Globals: nur reines JS plus ctx/tx.
// `_`-Präfix: keine Handler-Datei (siehe V4-HANDLERS.md, Laufzeit-Wunsch W1).

// ref: store_customer_commands.rs:1196-1217
export const CUSTOMER_ACCOUNT_STATUSES = ['active', 'inactive', 'archived'];
export const CUSTOMER_STAGES = [
  'prospect', 'onboarding', 'active', 'renewal', 'expansion', 'at_risk', 'churned', 'archived',
];
export const CUSTOMER_HEALTH_STATUSES = ['unknown', 'healthy', 'neutral', 'at_risk', 'critical'];
export const CUSTOMER_OPPORTUNITY_STAGES = [
  'qualification', 'proposal', 'negotiation', 'committed', 'closed_won', 'closed_lost',
];
export const CUSTOMER_OPPORTUNITY_TYPES = ['new_business', 'expansion', 'renewal'];
export const CUSTOMER_TASK_STATUSES = ['open', 'in_progress', 'completed', 'cancelled'];

// ---------------------------------------------------------------------------
// Fehler: Vertrag throw {code, message}

export function fail(code, message) {
  // eslint-disable-next-line no-throw-literal
  throw { code, message };
}

// ---------------------------------------------------------------------------
// JSON-Werte (serde_json-Semantik)

export function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function clone(value) {
  return value === undefined ? undefined : JSON.parse(JSON.stringify(value));
}

function getPath(value, path) {
  let cursor = value;
  for (const key of path) {
    if (!isObject(cursor) || !Object.prototype.hasOwnProperty.call(cursor, key)) return undefined;
    cursor = cursor[key];
  }
  return cursor;
}

// ref: store.rs:18292 outbound_string – String, getrimmt, nicht leer
export function str(value, path) {
  const cursor = getPath(value, path);
  if (typeof cursor !== 'string') return undefined;
  const trimmed = cursor.trim();
  return trimmed === '' ? undefined : trimmed;
}

// ref: store.rs:18312 outbound_required_string
export function requiredStr(value, path) {
  const result = str(value, path);
  if (result === undefined) fail('invalid', `${path.join('.')} is required`);
  return result;
}

// ref: store.rs:18304 outbound_first_string
export function firstString(values) {
  return values.find((value) => typeof value === 'string' && value !== '');
}

// ref: store_customer_commands.rs:1499 customers_i64 – nur Ganzzahlen
export function i64(value, path) {
  const cursor = getPath(value, path);
  return typeof cursor === 'number' && Number.isInteger(cursor) ? cursor : undefined;
}

// ref: store.rs:18285 outbound_object_payload
export function objectPayload(payload) {
  return isObject(payload) ? clone(payload) : {};
}

// ref: store.rs:18401 outbound_merge_fields – vorhandener Schlüssel (auch null) überschreibt
export function mergeFields(record, patch, keys) {
  if (!isObject(record) || !isObject(patch)) return;
  for (const key of keys) {
    if (Object.prototype.hasOwnProperty.call(patch, key) && patch[key] !== undefined) {
      record[key] = clone(patch[key]);
    }
  }
}

// ref: store.rs:18370
export function putDefaultString(record, key, fallback) {
  if (str(record, [key]) === undefined) record[key] = fallback;
}

// ref: store.rs:18376
export function putDefaultI64(record, key, fallback) {
  if (i64(record, [key]) === undefined) record[key] = fallback;
}

// ref: store_customer_commands.rs:1488
export function putDefaultBool(record, key, fallback) {
  if (typeof record[key] !== 'boolean') record[key] = fallback;
}

// ref: store.rs:18392
export function putDefaultObject(record, key) {
  if (!isObject(record[key])) record[key] = {};
}

// Rust to_ascii_lowercase: nur A-Z
export function asciiLower(value) {
  return String(value).replace(/[A-Z]/g, (c) => c.toLowerCase());
}

// ref: store_customer_commands.rs:1509 customers_refresh_search_text
export function refreshSearchText(record, fields) {
  record.search_text = asciiLower(
    fields.map((field) => str(record, [field])).filter((value) => value !== undefined).join(' '),
  );
}

// ref: store_customer_commands.rs:1519
export function contactDisplayName(contact) {
  const first = str(contact, ['first_name']) ?? '';
  const last = str(contact, ['last_name']) ?? '';
  const name = `${first} ${last}`.trim();
  return name === '' ? (str(contact, ['email']) ?? 'Kontakt') : name;
}

// ref: store_customer_commands.rs:1393
export function validateAllowed(label, value, allowed) {
  if (!allowed.includes(value)) fail('invalid', `${label} \`${value}\` is not supported`);
}

// ref: store_customer_commands.rs:1347
export function validateAccount(account) {
  validateAllowed('account_status', requiredStr(account, ['account_status']), CUSTOMER_ACCOUNT_STATUSES);
  validateAllowed('customer_stage', requiredStr(account, ['customer_stage']), CUSTOMER_STAGES);
  validateAllowed('health_status', requiredStr(account, ['health_status']), CUSTOMER_HEALTH_STATUSES);
}

// ref: store_customer_commands.rs:1366
export function validateOpportunity(opportunity) {
  validateAllowed('opportunity_type', requiredStr(opportunity, ['opportunity_type']), CUSTOMER_OPPORTUNITY_TYPES);
  validateAllowed('opportunity stage', requiredStr(opportunity, ['stage']), CUSTOMER_OPPORTUNITY_STAGES);
  const probability = i64(opportunity, ['probability']);
  if (probability !== undefined && (probability < 0 || probability > 100)) {
    fail('invalid', 'opportunity probability must be between 0 and 100');
  }
}

// ref: store_customer_commands.rs:1386
export function validateStageTransition(fromStage, toStage) {
  if ((fromStage === 'closed_won' || fromStage === 'closed_lost') && fromStage !== toStage) {
    fail('conflict', 'closed opportunities cannot move stage');
  }
}

// ---------------------------------------------------------------------------
// Befehl → IDs

// Übergang: v4-Befehle tragen kein record_id; v3-Befehle (UI-Builder index.js:3306ff.) schon.
function recordIdOf(command) {
  return typeof command?.record_id === 'string' ? command.record_id.trim() : undefined;
}

function payloadOf(command) {
  return command?.payload;
}

// ref: store.rs:18316 outbound_id_from_command
export function idFromCommand(command, payloadKeys, prefix) {
  const payload = payloadOf(command);
  const fromPayload = payloadKeys.map((key) => str(payload, [key])).find((v) => v !== undefined);
  const found = firstString([fromPayload, recordIdOf(command)]);
  if (found !== undefined) return found;
  if (typeof command?.id === 'string') return `${prefix}_${stableDigest(command.id)}`;
  return fail('invalid', 'record id is required');
}

// ref: store.rs:18340 outbound_required_from_payload_or_record
export function requiredFromPayloadOrRecord(command, payloadKeys, message) {
  const payload = payloadOf(command);
  const fromPayload = payloadKeys.map((key) => str(payload, [key])).find((v) => v !== undefined);
  const found = firstString([fromPayload, recordIdOf(command)]);
  if (found === undefined) fail('invalid', message);
  return found;
}

// ref: store.rs:18470 outbound_session_actor_id
export function actorId(command) {
  const id = command?.actor?.id;
  return typeof id === 'string' ? id : 'business-os-user';
}

// ---------------------------------------------------------------------------
// Speicher (db = ctx oder tx)

const META_FIELDS = ['_rev', '_deleted', '_meta', '_attachments'];

function stripMeta(doc) {
  const copy = clone(doc);
  for (const key of META_FIELDS) delete copy[key];
  return copy;
}

// ref: store.rs:18412 outbound_load_record – entfernte Dokumente (null) zählen nicht
export async function loadDoc(db, collection, id) {
  const doc = await db.get(collection, id);
  if (!doc || doc._deleted === true) return null;
  return stripMeta(doc);
}

// ref: store.rs:18436 outbound_load_required
export async function loadRequired(db, collection, id, message) {
  const doc = await loadDoc(db, collection, id);
  if (!doc) fail('not_found', message);
  return doc;
}

export async function requireExisting(db, collection, id, message) {
  await loadRequired(db, collection, id, message);
}

export async function ensureAbsent(db, collection, id, message) {
  if (await loadDoc(db, collection, id)) fail('conflict', message);
}

// ref: store_projections.rs:1397 upsert_business_record – setzt id und updated_at_ms auf der gespeicherten Kopie;
// _rev/_deleted verwaltet in v4 der Store. Rückgabe = der übergebene Stand ohne diese Zusätze (wie Rust .clone()).
export async function putRecord(db, collection, id, now, doc) {
  const stored = clone(doc);
  stored.id = id;
  stored.updated_at_ms = now;
  await db.put(collection, stored);
}

// ref: store_customer_commands.rs:1401 customers_validate_links
export async function validateLinks(db, record, requireAny) {
  const accountId = str(record, ['account_id']);
  const contactId = str(record, ['contact_id']);
  const opportunityId = str(record, ['opportunity_id']);
  if (requireAny && accountId === undefined && contactId === undefined && opportunityId === undefined) {
    fail('invalid', 'account_id, contact_id or opportunity_id is required');
  }
  if (accountId !== undefined) await requireExisting(db, 'customer_accounts', accountId, 'customer account not found');
  if (contactId !== undefined) await requireExisting(db, 'customer_contacts', contactId, 'customer contact not found');
  if (opportunityId !== undefined) {
    await requireExisting(db, 'customer_opportunities', opportunityId, 'customer opportunity not found');
  }
}

// ref: store_customer_commands.rs:1530
export async function touchAccountLastActivity(db, accountId, now) {
  if (typeof accountId !== 'string' || accountId.trim() === '') return;
  const account = await loadDoc(db, 'customer_accounts', accountId);
  if (!account) return;
  account.last_activity_at_ms = now;
  await putRecord(db, 'customer_accounts', accountId, now, account);
}

// ref: store_customer_commands.rs:1231 customers_write_activity
export async function writeActivity(db, command, now, activity) {
  const commandId = typeof command?.id === 'string' ? command.id : 'customers-command';
  const activityId = `act_${stableDigest(
    `${commandId}:${activity.activity_type}:${activity.linked_record_id ?? ''}`,
  )}`;
  let properties = activity.properties === undefined ? null : clone(activity.properties);
  if (!isObject(properties)) properties = { value: properties };
  properties.command_id = commandId;
  properties.command_type = command?.type;
  const record = {
    id: activityId,
    happens_at_ms: now,
    activity_type: activity.activity_type,
    name: activity.name,
    properties,
    actor_id: actorId(command),
    account_id: activity.account_id ?? '',
    contact_id: activity.contact_id ?? '',
    opportunity_id: activity.opportunity_id ?? '',
    linked_record_type: activity.linked_record_type ?? '',
    linked_record_id: activity.linked_record_id ?? '',
    linked_record_name: activity.linked_record_name ?? '',
    source: 'customers',
    source_record_id: commandId,
    is_deleted: false,
    created_at_ms: now,
  };
  await putRecord(db, 'customer_activities', activityId, now, record);
  return record;
}

// ref: store_customer_commands.rs:1290
export function writeTaskActivity(db, command, now, task, activityType, name) {
  return writeActivity(db, command, now, {
    activity_type: activityType,
    name,
    account_id: str(task, ['account_id']),
    contact_id: str(task, ['contact_id']),
    opportunity_id: str(task, ['opportunity_id']),
    linked_record_type: 'task',
    linked_record_id: str(task, ['id']),
    linked_record_name: str(task, ['title']),
    properties: { task: clone(task) },
  });
}

// ref: store_customer_commands.rs:1319
export function writeNoteActivity(db, command, now, note, activityType, name) {
  return writeActivity(db, command, now, {
    activity_type: activityType,
    name,
    account_id: str(note, ['account_id']),
    contact_id: str(note, ['contact_id']),
    opportunity_id: str(note, ['opportunity_id']),
    linked_record_type: 'note',
    linked_record_id: str(note, ['id']),
    linked_record_name: str(note, ['title']),
    properties: { note: clone(note) },
  });
}

// ---------------------------------------------------------------------------
// Ableitungen

// ref: store_customer_commands.rs:1627 customers_domain_from_website (Url::parse, WHATWG wie rust-url)
export function domainFromWebsite(value) {
  const raw = String(value ?? '').trim();
  if (raw === '') return undefined;
  const candidate = raw.includes('://') ? raw : `https://${raw}`;
  let host;
  if (typeof URL === 'function') {
    try {
      host = new URL(candidate).hostname;
    } catch {
      return undefined;
    }
  } else {
    // Rückfall ohne URL-Global (Laufzeit-Wunsch W2): Schema, Pfad, Nutzer, Port abtrennen.
    const rest = candidate.slice(candidate.indexOf('://') + 3);
    host = rest.split(/[/?#]/)[0];
    host = host.slice(host.lastIndexOf('@') + 1);
    if (!host.startsWith('[')) host = host.replace(/:\d*$/, '');
    host = asciiLower(host);
  }
  if (!host) return undefined;
  while (host.startsWith('www.')) host = host.slice(4); // trim_start_matches entfernt wiederholt
  return asciiLower(host);
}

// ref: store_customer_commands.rs:1643
export function splitName(value) {
  const parts = String(value ?? '').split(/\s+/).filter((part) => part !== '');
  if (parts.length === 0) return ['', ''];
  return [parts[0], parts.slice(1).join(' ')];
}

// serde_json-Display eines Werts (preserve_order aktiv: indexmap in Cargo.lock) ≈ JSON.stringify
export function jsonDisplay(value) {
  return JSON.stringify(value === undefined ? null : value);
}

// ---------------------------------------------------------------------------
// stable_digest = erste 24 Hex-Zeichen von SHA-256(UTF-8) (mission/channels/account_helpers.rs:277)

export function stableDigest(input) {
  return sha256Hex(String(input)).slice(0, 24);
}

function utf8Bytes(text) {
  const bytes = [];
  for (let i = 0; i < text.length; i += 1) {
    let code = text.charCodeAt(i);
    if (code >= 0xd800 && code <= 0xdbff && i + 1 < text.length) {
      const next = text.charCodeAt(i + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        code = 0x10000 + ((code - 0xd800) << 10) + (next - 0xdc00);
        i += 1;
      }
    }
    if (code >= 0xd800 && code <= 0xdfff) code = 0xfffd; // einzelnes Surrogat wie String::from_utf8_lossy
    if (code < 0x80) bytes.push(code);
    else if (code < 0x800) bytes.push(0xc0 | (code >> 6), 0x80 | (code & 63));
    else if (code < 0x10000) bytes.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 63), 0x80 | (code & 63));
    else {
      bytes.push(0xf0 | (code >> 18), 0x80 | ((code >> 12) & 63), 0x80 | ((code >> 6) & 63), 0x80 | (code & 63));
    }
  }
  return bytes;
}

const K = [
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

export function sha256Hex(text) {
  const bytes = utf8Bytes(text);
  const bitLength = bytes.length * 8;
  bytes.push(0x80);
  while (bytes.length % 64 !== 56) bytes.push(0);
  const high = Math.floor(bitLength / 0x100000000);
  const low = bitLength >>> 0;
  bytes.push((high >>> 24) & 255, (high >>> 16) & 255, (high >>> 8) & 255, high & 255);
  bytes.push((low >>> 24) & 255, (low >>> 16) & 255, (low >>> 8) & 255, low & 255);
  const h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
  const w = new Array(64);
  const rotr = (x, n) => (x >>> n) | (x << (32 - n));
  for (let offset = 0; offset < bytes.length; offset += 64) {
    for (let t = 0; t < 16; t += 1) {
      const j = offset + t * 4;
      w[t] = ((bytes[j] << 24) | (bytes[j + 1] << 16) | (bytes[j + 2] << 8) | bytes[j + 3]) >>> 0;
    }
    for (let t = 16; t < 64; t += 1) {
      const s0 = rotr(w[t - 15], 7) ^ rotr(w[t - 15], 18) ^ (w[t - 15] >>> 3);
      const s1 = rotr(w[t - 2], 17) ^ rotr(w[t - 2], 19) ^ (w[t - 2] >>> 10);
      w[t] = (w[t - 16] + s0 + w[t - 7] + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let t = 0; t < 64; t += 1) {
      const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const ch = (e & f) ^ (~e & g);
      const t1 = (hh + S1 + ch + K[t] + w[t]) >>> 0;
      const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (S0 + maj) >>> 0;
      hh = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    h[0] = (h[0] + a) >>> 0; h[1] = (h[1] + b) >>> 0; h[2] = (h[2] + c) >>> 0; h[3] = (h[3] + d) >>> 0;
    h[4] = (h[4] + e) >>> 0; h[5] = (h[5] + f) >>> 0; h[6] = (h[6] + g) >>> 0; h[7] = (h[7] + hh) >>> 0;
  }
  return h.map((x) => x.toString(16).padStart(8, '0')).join('');
}
