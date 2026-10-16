// Gemeinsame Fachlogik des Decision Hub (Port aus decision_hub.rs, Stand 4794af03d).
// Jede Funktion nennt ihr Rust-Vorbild. Kein Dateisystem, kein Netz.

export const COL_VORGAENGE = 'kundenpipeline_vorgaenge';
export const COL_ENTSCHEIDUNGEN = 'kundenpipeline_entscheidungen';
export const COL_PROJEKTE = 'kundenpipeline_projekte';
export const WRAP_WIDTH = 52;
export const AGENT_DECISION_TYPE = 'agent_escalation';
export const AGENT_DECISION_MIN_OPTIONS = 2;
export const AGENT_DECISION_MAX_OPTIONS = 8;
export const AGENT_DECISION_MAX_TITLE_CHARS = 160;
export const AGENT_DECISION_MAX_QUESTION_CHARS = 2_000;
export const AGENT_DECISION_MAX_CONTEXT_CHARS = 8_000;
export const AGENT_DECISION_MAX_OPTION_LABEL_CHARS = 120;
export const AGENT_DECISION_MAX_OPTION_DESCRIPTION_CHARS = 1_000;

// Fehler nach Handler-Vertrag: throw {code, message}.
export function fail(code, message) {
  throw { code, message };
}

const chars = (text) => Array.from(String(text));

// ref: decision_hub.rs kurz :1463
export function kurz(text, max) {
  const trimmed = String(text ?? '').trim();
  const list = chars(trimmed);
  if (list.length <= max) return trimmed;
  return `${list.slice(0, Math.max(0, max - 1)).join('')}…`;
}

// ref: decision_hub.rs wrap_text :1435
export function wrapText(text, width) {
  const out = [];
  for (const paragraph of String(text ?? '').split('\n')) {
    const trimmed = paragraph.trim();
    if (!trimmed) {
      out.push('');
      continue;
    }
    let line = '';
    for (const word of trimmed.split(/\s+/)) {
      if (line && chars(line).length + 1 + chars(word).length > width) {
        out.push(line);
        line = '';
      }
      if (line) line += ' ';
      line += word;
    }
    if (line) out.push(line);
  }
  while (out.length && out[out.length - 1] === '') out.pop();
  return out;
}

// ref: decision_hub.rs deterministic_id :1375 (FNV-1a 64 ueber UTF-8-Bytes)
export function deterministicId(prefix, key) {
  let hash = 1469598103934665603n;
  for (const byte of utf8(key)) {
    hash ^= BigInt(byte);
    hash = (hash * 1099511628211n) & 0xffffffffffffffffn;
  }
  const sanitized = chars(key)
    .filter((ch) => /^[A-Za-z0-9._-]$/.test(ch))
    .slice(0, 80)
    .join('');
  return `${prefix}-${sanitized}-${hash.toString(16).padStart(16, '0')}`;
}

export function utf8(text) {
  const out = [];
  for (const ch of String(text)) {
    let cp = ch.codePointAt(0);
    if (cp >= 0xd800 && cp <= 0xdfff) cp = 0xfffd; // einzelne Surrogate wie Rust-Lossy
    if (cp < 0x80) out.push(cp);
    else if (cp < 0x800) out.push(0xc0 | (cp >> 6), 0x80 | (cp & 63));
    else if (cp < 0x10000) out.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
    else out.push(0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 63), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
  }
  return out;
}

// ref: decision_hub.rs sha256_hex :1147 (reines JS, weil der vm-Kontext kein node:crypto hat)
const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);
export function sha256Hex(text) {
  const bytes = utf8(text);
  const bitLen = bytes.length * 8;
  bytes.push(0x80);
  while (bytes.length % 64 !== 56) bytes.push(0);
  const hi = Math.floor(bitLen / 0x100000000);
  const lo = bitLen >>> 0;
  bytes.push((hi >>> 24) & 255, (hi >>> 16) & 255, (hi >>> 8) & 255, hi & 255);
  bytes.push((lo >>> 24) & 255, (lo >>> 16) & 255, (lo >>> 8) & 255, lo & 255);
  const h = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);
  const w = new Uint32Array(64);
  const rotr = (x, n) => (x >>> n) | (x << (32 - n));
  for (let off = 0; off < bytes.length; off += 64) {
    for (let i = 0; i < 16; i += 1) {
      const j = off + i * 4;
      w[i] = (bytes[j] << 24) | (bytes[j + 1] << 16) | (bytes[j + 2] << 8) | bytes[j + 3];
    }
    for (let i = 16; i < 64; i += 1) {
      const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
      const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) >>> 0;
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let i = 0; i < 64; i += 1) {
      const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const ch = (e & f) ^ (~e & g);
      const t1 = (hh + S1 + ch + K[i] + w[i]) >>> 0;
      const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (S0 + maj) >>> 0;
      hh = g; g = f; f = e; e = (d + t1) >>> 0; d = c; c = b; b = a; a = (t1 + t2) >>> 0;
    }
    h[0] = (h[0] + a) >>> 0; h[1] = (h[1] + b) >>> 0; h[2] = (h[2] + c) >>> 0; h[3] = (h[3] + d) >>> 0;
    h[4] = (h[4] + e) >>> 0; h[5] = (h[5] + f) >>> 0; h[6] = (h[6] + g) >>> 0; h[7] = (h[7] + hh) >>> 0;
  }
  return [...h].map((x) => x.toString(16).padStart(8, '0')).join('');
}

// ref: decision_hub.rs sha256_id :1143
export const sha256Id = (prefix, text) => `${prefix}-${sha256Hex(text)}`;

const str = (value) => (typeof value === 'string' ? value : undefined);

// ref: decision_hub.rs optional_trimmed :1121
export function optionalTrimmed(value, field, maxChars) {
  if (value === null || typeof value !== 'object' || !Object.prototype.hasOwnProperty.call(value, field)) return undefined;
  const raw = value[field];
  if (raw === null || raw === undefined) return undefined;
  if (typeof raw !== 'string') fail('invalid', `${field} must be a string`);
  const trimmed = raw.trim();
  if (chars(trimmed).length > maxChars) fail('invalid', `${field} exceeds the ${maxChars} character limit`);
  return trimmed;
}

// ref: decision_hub.rs required_trimmed :1115
export function requiredTrimmed(value, field, maxChars) {
  const v = optionalTrimmed(value, field, maxChars);
  if (!v) fail('invalid', `${field} is required`);
  return v;
}

// ref: decision_hub.rs load_any_record :1161 – v4 kennt nur noch einen Speicher;
// der Rueckfall auf `business_records` entfaellt. Geloeschte Datensaetze zaehlen nicht.
export async function loadAnyRecord(ctx, collection, id) {
  if (!id) return null;
  const record = await ctx.get(collection, id);
  if (!record || record.is_deleted === true) return null;
  return record;
}

// ref: decision_hub.rs audit_entry :1334 / push_audit :1338 (nur wenn audit_json ein Array ist)
export const auditEntry = (now, aktion, akteur, kanal) => ({ zeit_ms: now, aktion, akteur, kanal });
export function pushAudit(vorgang, now, aktion, akteur, kanal) {
  if (Array.isArray(vorgang.audit_json)) vorgang.audit_json.push(auditEntry(now, aktion, akteur, kanal));
}

// ref: decision_hub.rs sender_address_of :1344
export const senderAddressOf = (vorgang) => str(vorgang?.quelle_json?.absender) ?? '';

// ref: decision_hub.rs clean_body_or :1352
export function cleanBodyOr(vorgang) {
  const body = str(vorgang?.quelle_json?.body_clean);
  if (body !== undefined && body.trim()) return body;
  return str(vorgang?.quelle_json?.betreff) ?? '';
}

// ref: decision_hub.rs workspace_decision_user_ids :1317 – Nutzerliste ist Plattform-
// wissen (business_users). CAP-3b #844 caps/directory.rs: identity.users.list({active, roles, order})
// → {users:[{user_id, display_name, role, active}]}; Fehler -> [] wie in Rust.
export async function workspaceDecisionUserIds(ctx) {
  try {
    const result = await ctx.cap.identity.users.list({
      active: true,
      roles: ['chef', 'admin'],
      order: ['created_at_ms:asc', 'user_id:asc'],
    });
    return (Array.isArray(result?.users) ? result.users : [])
      .map((u) => (typeof u === 'string' ? u : u?.user_id))
      .filter((id) => typeof id === 'string');
  } catch {
    return [];
  }
}

// ref: decision_hub.rs decision_record :1278
export function decisionRecord(decisionId, vorgangId, typ, titel, zeilen, userIds, now) {
  const owner = userIds[0] ?? '';
  return {
    id: decisionId,
    vorgang_id: vorgangId,
    typ,
    titel,
    title: titel,
    zeilen_json: zeilen,
    detail_seiten_json: [],
    aktionen_json: [],
    backing_ref: '',
    status: 'offen',
    antwort_json: {},
    owner_user_id: owner,
    assigned_user_id: owner,
    participant_ids: userIds,
    is_deleted: false,
    created_at_ms: now,
    updated_at_ms: now,
  };
}

// Rust: session.user.id, sonst ein fester Ersatzname je Handler.
export const actorId = (command, fallback) => {
  const id = command?.actor?.id;
  return typeof id === 'string' && id ? id : fallback;
};

// Rust upsert_projection_record schreibt den ganzen Datensatz.
export async function upsert(ctx, collection, doc) {
  const result = await ctx.put(collection, doc);
  if (result && result.status === 'rejected') fail(result.reason === 'not_found' ? 'not_found' : 'conflict', `write to ${collection} rejected: ${result.reason}`);
  return result;
}

// ref: decision_hub.rs agent_decision_response :1086
export function agentDecisionResponse(decision, now) {
  const persisted = str(decision.status);
  const expired = persisted === 'offen' && Number.isInteger(decision.expires_at_ms) && decision.expires_at_ms <= now;
  let status;
  if (expired) status = 'expired';
  else if (persisted === 'offen') status = 'open';
  else if (persisted === 'entschieden' || persisted === 'abgelehnt') status = 'resolved';
  else if (persisted === 'abgelaufen') status = 'expired';
  else if (persisted !== undefined) status = persisted;
  else status = 'open';
  return {
    ok: true,
    decision_id: decision.id ?? null,
    status,
    resolution: status === 'resolved' ? (decision.antwort_json ?? {}) : null,
    correlation: decision.correlation_json ?? {},
    updated_at_ms: decision.updated_at_ms ?? null,
  };
}

// ref: decision_hub.rs resolve_agent_decision :441 – geteilt von decision.resolve und decision.answer.
export async function resolveAgentDecision(ctx, payload, command) {
  const decisionId = requiredTrimmed(payload, 'entscheidung_id', 180);
  const optionId = requiredTrimmed(payload, 'option_id', 120);
  const comment = optionalTrimmed(payload, 'comment', 2_000);
  const channel = optionalTrimmed(payload, 'kanal', 80) ?? 'desktop';
  const decision = await loadAnyRecord(ctx, COL_ENTSCHEIDUNGEN, decisionId);
  if (!decision) fail('not_found', `unknown Entscheidung \`${decisionId}\``);
  if (decision.typ !== AGENT_DECISION_TYPE) {
    fail('invalid', 'kundenpipeline.decision.resolve only accepts agent_escalation decisions');
  }
  const option = (Array.isArray(decision.aktionen_json) ? decision.aktionen_json : []).find((o) => o?.wert === optionId);
  if (!option) fail('invalid', `unknown Decision Hub option \`${optionId}\``);
  if (decision.status === 'entschieden') {
    const same = decision.antwort_json?.option_id === optionId && decision.antwort_json?.comment === (comment ?? '');
    if (!same) fail('conflict', 'decision_resolution_conflict: decision was already resolved differently');
    return agentDecisionResponse(decision, ctx.now());
  }
  if (decision.status !== 'offen') fail('conflict', 'Decision Hub decision is not open');
  const now = ctx.now();
  if (Number.isInteger(decision.expires_at_ms) && !(decision.expires_at_ms > now)) {
    fail('expired', 'Decision Hub decision has expired');
  }
  const actor = actorId(command, 'owner');
  decision.status = 'entschieden';
  decision.updated_at_ms = now;
  decision.antwort_json = {
    option_id: optionId,
    wert: option.wert ?? null,
    label: option.label ?? null,
    comment: comment ?? '',
    kanal: channel,
    akteur: actor,
    resolved_at_ms: now,
    zeit_ms: now,
  };
  await upsert(ctx, COL_ENTSCHEIDUNGEN, decision);
  return agentDecisionResponse(decision, now);
}

// ---------------------------------------------------------------------------
// Eingang (Strang SKL-B4): Projektion eingehender Mails → Vorgänge, Absender-Routing, Triage-Antwort.

// ref: decision_hub.rs strip_mail_ballast :1391 – Grüße, Signaturen und zitierte Verläufe entfernen.
const SIGNATURE_MARKERS = ['mit freundlichen grüßen', 'viele grüße', 'beste grüße', 'freundliche grüße', 'liebe grüße', 'best regards', 'kind regards'];
const GREETINGS = ['hi ', 'hallo', 'guten tag', 'sehr geehrte', 'dear '];
export function stripMailBallast(text) {
  const body = String(text ?? '').replaceAll('\r\n', '\n');
  // Rust str::lines(): kein leeres Element nach einem abschließenden \n, \r am Zeilenende fällt weg.
  const raw = body.split('\n');
  if (raw.length && raw[raw.length - 1] === '') raw.pop();
  const lines = [];
  for (const rawLine of raw) {
    const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine;
    const lowered = line.trim().toLowerCase();
    if (SIGNATURE_MARKERS.some((marker) => lowered.startsWith(marker))) break;
    if (lowered.startsWith('-----original') || (lowered.startsWith('am ') && lowered.includes(' schrieb '))) break;
    lines.push(line);
  }
  let start = 0;
  if (lines.length) {
    const first = lines[0].trim();
    const lowered = first.toLowerCase();
    // Rust: `first.trim().len() < 70` zählt UTF-8-Bytes.
    if (GREETINGS.some((greeting) => lowered.startsWith(greeting)) && utf8(first).length < 70) {
      start = 1;
      while (start < lines.length && !lines[start].trim()) start += 1;
    }
  }
  return lines.slice(start).join('\n').trim();
}

// ref: decision_hub.rs string_list :1262
function stringList(value) {
  return Array.isArray(value)
    ? value.filter((item) => typeof item === 'string').map((item) => item.trim().toLowerCase()).filter(Boolean)
    : [];
}

/**
 * ref: decision_hub.rs route_sender :1224 – exakte Adresse → Treffer (sofort), sonst erste Domain → Vorschlag.
 * Abweichung (Befund B2): Rust las `adressen_json`/`domains_json`/`aktiv`, das Schema und die UI schreiben
 * `adressen`/`domains`/`active`. Gelesen wird der Schema-Name, sonst der Rust-Name.
 */
export function routeSender(projekte, sender) {
  const address = String(sender ?? '').trim().toLowerCase();
  const domain = address.split('@')[1] ?? '';
  let vorschlag = null;
  for (const raw of projekte) {
    const aktiv = typeof raw?.active === 'boolean' ? raw.active : raw?.aktiv;
    if (aktiv === false) continue;
    const projekt = {
      id: str(raw?.id) ?? '',
      name: str(raw?.name) ?? '',
      code_projekt: str(raw?.code_projekt) ?? '',
      adressen: stringList(raw?.adressen ?? raw?.adressen_json),
      domains: stringList(raw?.domains ?? raw?.domains_json),
    };
    if (projekt.adressen.includes(address)) return { treffer: projekt, vorschlag: null };
    if (domain && !vorschlag && projekt.domains.includes(domain)) vorschlag = projekt;
  }
  return { treffer: null, vorschlag };
}

// ref: decision_hub.rs non_empty :1366
export const nonEmpty = (value, fallback) => (String(value ?? '').trim() ? value : fallback);

// ref: store.rs:23796 extract_json_object – erstes vollständiges JSON-Objekt einer Agentenantwort.
export function extractJsonObject(text) {
  const trimmed = String(text ?? '').trim();
  const isObj = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
  try {
    const value = JSON.parse(trimmed);
    if (isObj(value)) return value;
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
          return isObj(value) ? value : undefined;
        } catch {
          return undefined;
        }
      }
    }
  }
  return undefined;
}

/** Text einer Agentenantwort aus dem Rückruf-Payload (result als Text oder {text}). */
export function resultText(result) {
  if (typeof result === 'string') return result;
  if (typeof result?.text === 'string') return result.text;
  return result == null ? '' : JSON.stringify(result);
}

/** Fehlermeldung eines on_failed-Rückrufs: error.message, sonst result.error/message, sonst „task <status>“. */
export function failureMessage(payload) {
  const hit = [payload?.error?.message, payload?.error, payload?.result?.error, payload?.result?.message, payload?.status_note]
    .find((value) => typeof value === 'string' && value.trim() !== '');
  return hit ? hit.trim() : `task ${payload?.status ?? 'failed'}`;
}
