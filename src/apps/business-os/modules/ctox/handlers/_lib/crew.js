// Treue Portierung der Prüf-Helfer aus crew_commands.rs und src/core/crew/mod.rs
// (Stand 4794af03d). Reine Funktionen bis auf die Store-Helfer unten (lesen nur
// über den übergebenen ctx/Transaktions-Store).
import { charCount, invalid, isPlainObject } from './errors.js';

// Crew-Register, Lernsätze, Wesen-Wissen und Zuordnungen sind ctox-App-Collections
// (CATALOG.md „Kein crew-Primitiv“). Früher Kern-Tabellen crew_members,
// crew_member_learnings, LCM-Continuity, communication_routing_state.crew_assigned_member_id.
export const CREW_MEMBERS = 'ctox_crew_members';
export const CREW_LEARNINGS = 'ctox_crew_learnings';
export const CREW_ASSIGNMENTS = 'ctox_crew_assignments';

export const CREW_SHAPES = ['round', 'square', 'triangle', 'blob'];
// No neutral grey: it is the "no member yet" creature (crew_commands.rs).
export const CREW_COLORS = ['#1685ee', '#00aa9a', '#e0a82e', '#7c6df2', '#e97255', '#34a26f'];

// crew::prose_line – split_whitespace + join(" ")
export function proseLine(text) {
  return String(text).split(/\s+/u).filter(Boolean).join(' ');
}

// crew::normalized
export function normalized(text) {
  return proseLine(text).toLowerCase();
}

// Rust-regex `\w` und `\b` sind Unicode-bewusst; JS-`\w` ist auch mit /u nur
// ASCII. W bildet Rusts Wortzeichen nach, B die Wortgrenze per Lookaround.
const W = '[\\p{L}\\p{M}\\p{Nd}\\p{Pc}]';
const WD = `(?:${W}|[.-])`; // [\w.-]
const B = `(?:(?<=${W})(?!${W})|(?<!${W})(?=${W}))`;
const SENSITIVE = new RegExp(
  [
    '/users/',
    '/home/',
    '/tmp/',
    '/var/',
    '[a-z]:\\\\',
    '~/',
    '\\./',
    `(?:^|\\s)/${WD}+`,
    `\\\\+${WD}+\\\\${WD}+`,
    `${B}${WD}+(?:/${WD}+)+\\.[a-z0-9]+${B}`,
    `${B}(?:AKIA|ghp_|eyJ|sk-)`,
    `${B}(?:${W}|-)*key(?:${W}|-)*\\s*=`,
    `(?:${W}|[.+-])+@${WD}+\\.[a-z]{2,}`,
  ].join('|'),
  'iu',
);
const SENSITIVE_WORDS = [
  'bearer ',
  'api_key',
  'api-key',
  'password',
  'passwd',
  'secret',
  'token=',
  'token:',
  'passwort',
  'authorization',
  'credential',
  'pwd=',
  'private key',
  'ctox_crew_soul',
];

// crew::safe_prose – "Store only prose."
export function safeProse(text, limit) {
  if (typeof text !== 'string') return false;
  const prose = proseLine(text);
  const lower = prose.toLowerCase();
  return (
    text.trim() !== '' &&
    charCount(text) <= limit &&
    !text.includes('\0') &&
    !SENSITIVE.test(prose) &&
    ![...text].some((c) => /\p{Cc}/u.test(c) && !/\s/u.test(c)) &&
    !SENSITIVE_WORDS.some((s) => lower.includes(s))
  );
}

// crew_commands.rs::text – Pflichttext, getrimmt, ≤ 200 Zeichen.
export function requireText(payload, key) {
  const raw = payload?.[key];
  const value = typeof raw === 'string' ? raw.trim() : '';
  if (!value) throw invalid(`${key} is required`);
  if (charCount(value) > 200) throw invalid(`${key} too long`);
  return value;
}

const SOUL_AXES = [
  'gruendlichkeit_vs_tempo',
  'vorsicht_vs_mut',
  'knapp_vs_ausfuehrlich',
  'regeltreu_vs_kreativ',
  'nachfragen_vs_annehmen',
];
const SOUL_FIELDS = [...SOUL_AXES, 'sketch', 'voice'];

// serde_json::from_value::<Soul> (deny_unknown_fields, u8-Achsen) +
// Soul::normalize + Soul::validate.
export function parseSoul(value) {
  if (!isPlainObject(value)) throw invalid('invalid crew soul');
  for (const key of Object.keys(value)) {
    if (!SOUL_FIELDS.includes(key)) throw invalid(`unknown field \`${key}\` in crew soul`);
  }
  const soul = {};
  for (const axis of SOUL_AXES) {
    const v = value[axis];
    if (!Number.isInteger(v) || v < 0 || v > 255) throw invalid(`invalid crew soul field ${axis}`);
    soul[axis] = v;
  }
  for (const key of ['sketch', 'voice']) {
    if (typeof value[key] !== 'string') throw invalid(`invalid crew soul field ${key}`);
    soul[key] = value[key];
  }
  soul.sketch = proseLine(soul.sketch);
  const voiceStops = (soul.voice.match(/[.!?]/g) || []).length;
  if (
    SOUL_AXES.some((axis) => soul[axis] > 100) ||
    charCount(soul.sketch) > 600 ||
    charCount(soul.voice) > 200 ||
    soul.voice.trim() === '' ||
    /[\n\r]/.test(soul.voice) ||
    voiceStops > 1 ||
    (soul.sketch !== '' && !safeProse(soul.sketch, 600))
  ) {
    throw invalid('invalid crew soul: axes must be 0–100, sketch ≤600 and voice ≤200 characters');
  }
  return soul;
}

const SPECIALTY_FIELDS = ['modules', 'command_types', 'skills', 'tags'];

// crew_commands.rs::specialties (serde default + deny_unknown_fields).
export function parseSpecialties(value) {
  if (!isPlainObject(value)) throw invalid('invalid specialties');
  for (const key of Object.keys(value)) {
    if (!SPECIALTY_FIELDS.includes(key)) throw invalid(`unknown field \`${key}\` in specialties`);
  }
  const out = {};
  for (const key of SPECIALTY_FIELDS) {
    const list = value[key] === undefined ? [] : value[key];
    if (!Array.isArray(list) || !list.every((s) => typeof s === 'string')) {
      throw invalid('invalid specialties');
    }
    if (!(list.length <= 20 && list.every((s) => safeProse(s, 100)))) throw invalid('invalid specialties');
    out[key] = [...list];
  }
  return out;
}

// crew_commands.rs::audit_payload – nie beliebige Payload-Werte kopieren.
export function crewAuditPayload(payload) {
  const out = {};
  for (const key of ['member_id', 'learning_id', 'task_id', 'name', 'shape', 'color', 'kind', 'mode']) {
    const value = payload?.[key];
    if (typeof value === 'string' && safeProse(value, 200)) out[key] = value;
  }
  if (typeof payload?.archived === 'boolean') out.archived = payload.archived;
  return out;
}

export function defaultStats() {
  return {
    tasks_total: 0,
    succeeded: 0,
    failed: 0,
    review_passed: 0,
    review_rejected: 0,
    avg_elapsed_ms: 0,
    last_active_at: null,
  };
}

// ref: src/core/crew/mod.rs:283-285 (members) – Rust `ORDER BY id LIMIT 1000`.
export async function listMembers(store) {
  return store.read(CREW_MEMBERS, { sort: [{ id: 'asc' }], limit: 1000 });
}

// Millisekunden aus dem ISO-Zeitstempel, den runCrewControl übergibt.
export function isoMillis(iso) {
  return Date.parse(iso);
}

// crew_commands.rs::control – Audit „requested“ (hart), Ausführung, Audit
// „succeeded|failed“ (verlustbehaftet). Rückgabe {ok:true, result}.
export async function runCrewControl(command, ctx, body) {
  const actor = command.actor?.id ?? null;
  const audit = crewAuditPayload(command.payload || {});
  await ctx.cap.agent.flow.record({
    event_kind: 'crew.control',
    title: command.type,
    metadata: { actor, command_type: command.type, stage: 'requested', payload: audit },
  });
  let ok = false;
  try {
    const result = await body(command.payload || {}, new Date(ctx.now()).toISOString());
    ok = true;
    return { ok: true, result };
  } finally {
    try {
      await ctx.cap.agent.flow.record({
        event_kind: 'crew.control',
        title: command.type,
        metadata: {
          actor,
          command_type: command.type,
          stage: ok ? 'succeeded' : 'failed',
          ok,
          payload: audit,
        },
      });
    } catch (error) {
      ctx.log('warn', 'crew.control result audit failed', { error: String(error?.message || error) });
    }
  }
}
