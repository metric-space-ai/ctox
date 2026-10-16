// Origin: CTOX
// License: Apache-2.0
// IoT-Fachmodell der iot-App (Strang IOTAPP, Shell v4). Ersetzt die Delegation an die Core-Capabilities
// ctox.iot.* (vormals handlers/_iot.js → Engine im Core). Wahrheit sind jetzt die eigenen iot_*-Collections;
// der Core liefert nur OS-Primitive (net.ingest, dataframe, model, agent.task) – siehe V4-HANDLERS.md.
// Fachliche Referenz: src/core/iot/{commands,model,store,alarms,conditions,watcher,widget_runtime}.rs @ main@4794af03d.
import { fail, isObject } from './_base.js';

// ref: commands.rs:42-50 + :278-313 (iot_dashboards/iot_widgets)
export const C = Object.freeze({
  assets: 'iot_assets', attributes: 'iot_attributes', assetTypes: 'iot_asset_types', alarms: 'iot_alarms',
  rulesets: 'iot_rulesets', agents: 'iot_agents', agentStatus: 'iot_agent_status', dashboards: 'iot_dashboards',
  widgets: 'iot_widgets', datapoints: 'iot_datapoints',
});
// Zeitreihen-Tabelle (Telemetrie-Historie) über knowledge.query – siehe _timeseries.js.
export { TIMESERIES_TABLE } from './_timeseries.js';
// ref: projector.rs:67 MAX_ATTRIBUTES_PER_ASSET
export const MAX_ATTRIBUTES_PER_ASSET = 1000;
// ref: widget_runtime.rs:35 WATCHER_LOOKBACK_MS
export const WATCHER_LOOKBACK_MS = 24 * 60 * 60 * 1000;
export const DESERIALIZE = 'failed to deserialize iot command payload';

/**
 * Realm der Sitzung – nie aus dem Payload. ref: commands.rs:567-572 session_realm (Phase 2: immer "master").
 * Auth-Gate ref: commands.rs:2395-2400 – im v4-Vertrag erzwingt der Core command_rights vor dem Handler; hier bleibt
 * nur die Pflicht eines Akteurs.
 */
export function sessionRealm(command) {
  if (!isObject(command?.actor)) fail('forbidden', 'iot: forbidden');
  return 'master';
}

// ---------------------------------------------------------------- Payload (serde-Abbild)

/**
 * Prüft das Payload wie serde_json::from_value::<Req>: Pflichtfelder vorhanden und typrichtig, Option-Felder
 * null|fehlend|typrichtig, `bool=true` = #[serde(default = "default_true")]. Fehler → invalid DESERIALIZE.
 * ref: commands.rs:2511 parse_payload; Req-Strukturen :90-221
 */
export function parsePayload(command, spec) {
  const payload = command?.payload;
  if (!isObject(payload)) fail('invalid', DESERIALIZE);
  const out = {};
  for (const [key, rule] of Object.entries(spec)) {
    const value = payload[key];
    const optional = rule.endsWith('?');
    const base = rule.replace(/\?$/, '').replace(/=.*$/, '');
    const absent = value === undefined || value === null;
    if (absent) {
      if (rule === 'bool=true') { out[key] = true; continue; }
      if (rule === 'any') { out[key] = null; continue; }
      if (!optional) fail('invalid', DESERIALIZE, { path: key });
      out[key] = null;
      continue;
    }
    const ok = base === 'string' ? typeof value === 'string'
      : base === 'bool' ? typeof value === 'boolean'
        : base === 'i64' ? Number.isInteger(value)
          : base === 'object' ? isObject(value)
            : true;
    if (!ok) fail('invalid', DESERIALIZE, { path: key });
    out[key] = value;
  }
  return out;
}

/** Feld-Prüfung der Einzel-Arme im Dispatcher: `<type> requires <field>`. ref: commands.rs:2410-2490 */
export function requireField(command, field, kind) {
  const payload = isObject(command?.payload) ? command.payload : {};
  const value = payload[field];
  const ok = kind === 'string' ? typeof value === 'string' : kind === 'bool' ? typeof value === 'boolean' : Number.isInteger(value);
  if (!ok) fail('invalid', `${command.type} requires ${field}`);
  return value;
}

/** Rule-/Agent-`data` (serde Value, default null): die Collection führt `data` als Objekt → null/fehlend = {}. */
export function dataObject(value) {
  if (value === null || value === undefined) return {};
  if (!isObject(value)) fail('invalid', DESERIALIZE, { path: 'data' });
  return value;
}

// ---------------------------------------------------------------- IDs, Hilfen

const BASE62 = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
/** 22-stellige Base62-ID aus einer UUID. ref: model.rs:282-307 generate_id/base62_encode_u128 */
export function generateId(ctx) {
  const uuid = typeof ctx?.uuid === 'function' ? ctx.uuid() : globalThis.crypto.randomUUID();
  let value = BigInt(`0x${uuid.replace(/-/g, '')}`);
  const chars = Array(22).fill('0');
  let idx = 22;
  while (value > 0n && idx > 0) {
    idx -= 1;
    chars[idx] = BASE62[Number(value % 62n)];
    value /= 62n;
  }
  return chars.join('');
}

/** Option-Felder: null → weglassen (Collections führen sie als string/number, nicht nullable). */
export function compact(doc) {
  const out = {};
  for (const [key, value] of Object.entries(doc)) if (value !== null && value !== undefined) out[key] = value;
  return out;
}

/** Rust `format!("{:08}", n)` / `{:020}` – Vorzeichen zählt zur Breite. */
export function zeroPad(n, width) {
  const abs = String(Math.abs(n)).padStart(n < 0 ? width - 1 : width, '0');
  return n < 0 ? `-${abs}` : abs;
}

/** Schreiben einer Zeile; created_at_ms bleibt beim Update erhalten. */
export async function writeRow(ctx, collection, id, payload, existing = undefined) {
  const prior = existing === undefined ? await ctx.get(collection, id) : existing;
  const now = ctx.now();
  const doc = compact({ ...payload, id, created_at_ms: prior?.created_at_ms ?? now, updated_at_ms: now });
  await ctx.put(collection, doc);
  return { collection, id };
}

export async function removeRow(ctx, collection, id) {
  if (!(await ctx.get(collection, id))) return null;
  await ctx.remove(collection, id);
  return { collection, id };
}

/** EngineOutcome.into_value: Fachergebnis + projections[] (Liste der geschriebenen Zeilen). ref: commands.rs:67-86 */
export const outcome = (result, projections) => ({ ...result, projections: projections.filter(Boolean) });

/** Realm-Schutz für bestehende Datensätze: fremder Realm = „nicht gefunden“. ref: commands.rs:617-623 u. a. */
export function ensureRealm(existing, realm, message) {
  if (existing && existing.realm !== realm) fail('not_found', message);
}

// ---------------------------------------------------------------- Assets/Attribute

const BASE_TYPES = ['Number', 'Boolean', 'Text', 'Object', 'Array', 'GeoPoint'];

/** AssetTypeInfo deserialisieren. ref: model.rs:97-125 */
export function parseAssetTypeInfo(value) {
  if (value === null || value === undefined) return null;
  const bad = () => fail('invalid', DESERIALIZE, { path: 'asset_type_info' });
  if (!isObject(value) || typeof value.asset_type !== 'string' || !Array.isArray(value.attributes)) bad();
  const attributes = value.attributes.map((d) => {
    if (!isObject(d) || typeof d.name !== 'string' || !isObject(d.value_descriptor)) bad();
    const vd = d.value_descriptor;
    if (typeof vd.name !== 'string' || !BASE_TYPES.includes(vd.base_type)) bad();
    if (d.meta !== undefined && d.meta !== null && !isObject(d.meta)) bad();
    return {
      name: d.name,
      value_descriptor: {
        name: vd.name, base_type: vd.base_type, array_dimensions: Number.isInteger(vd.array_dimensions) ? vd.array_dimensions : 0,
        constraints: Array.isArray(vd.constraints) ? vd.constraints : [], units: vd.units ?? null, format: vd.format ?? null,
      },
      meta: isObject(d.meta) ? d.meta : {},
    };
  });
  return { asset_type: value.asset_type, attributes };
}

/** Attribute::new + merge_descriptor_meta_once. ref: model.rs:151-216 */
export function attributeFromDescriptor(descriptor) {
  return { name: descriptor.name, value_type: descriptor.value_descriptor.base_type, value: null, timestamp: 0, meta: { ...descriptor.meta } };
}

/** Asset::new_with_type. ref: model.rs:257-279 */
export function newAssetWithType(id, realm, assetType, name, info) {
  const attributes = {};
  for (const d of info.attributes) attributes[d.name] = attributeFromDescriptor(d);
  return { id, parent_id: null, realm, asset_type: assetType, name, path: [], attributes };
}

/** ref: commands.rs:429-441 asset_row */
export const assetRow = (asset) => ({
  realm: asset.realm, parent_id: asset.parent_id, asset_type: asset.asset_type, name: asset.name, data: asset,
  index_text: asset.name.toLowerCase(), sort_key: asset.name,
});

export const attributeRecordId = (assetId, name) => `${assetId}:${name}`; // ref: commands.rs:443-445

/** ref: commands.rs:447-466 attribute_row */
export function attributeRow(asset, name, extra = {}) {
  const attr = asset.attributes?.[name];
  if (!attr) return null;
  const valueType = attr.value_type ?? '';
  return {
    realm: asset.realm, asset_id: asset.id, attribute_name: attr.name, value_type: valueType, timestamp_ms: attr.timestamp,
    data: attr, index_text: attr.name.toLowerCase(), sort_key: attr.name, status_key: valueType, ...extra,
  };
}

/** ref: commands.rs:468-478 asset_type_row */
export const assetTypeRow = (info) => ({
  asset_type: info.asset_type, attribute_count: info.attributes.length, data: info,
  index_text: info.asset_type.toLowerCase(), sort_key: info.asset_type,
});

/** Sortierte Attributnamen (BTreeMap-Reihenfolge). */
export const attributeNames = (asset) => Object.keys(asset.attributes ?? {}).sort();

/** ref: model.rs:333-342 normalize_event_timestamp */
export function normalizeEventTimestamp(ts, systemTimeMs) {
  if (!Number.isInteger(ts) || ts <= 0) return systemTimeMs;
  return ts > systemTimeMs ? systemTimeMs : ts;
}

/** ref: model.rs:346-395 coerce_value */
export function coerceValue(value, target) {
  if (value === null) return null;
  const cannot = (what) => fail('invalid', `cannot coerce ${what} to ${target}`);
  switch (target) {
    case 'Number':
      if (typeof value === 'number') return value;
      if (typeof value === 'boolean') return value ? 1.0 : 0.0;
      if (typeof value === 'string') {
        const n = Number(value.trim() === '' ? NaN : value);
        if (Number.isFinite(n)) return n;
        return cannot(`'${value}'`);
      }
      return cannot(JSON.stringify(value));
    case 'Boolean':
      if (typeof value === 'boolean') return value;
      if (typeof value === 'number') return value !== 0;
      if (typeof value === 'string') {
        if (['true', 'TRUE', '1'].includes(value)) return true;
        if (['false', 'FALSE', '0'].includes(value)) return false;
        return cannot(`'${value}'`);
      }
      return cannot(JSON.stringify(value));
    case 'Text':
      if (typeof value === 'string') return value;
      if (typeof value === 'number' || typeof value === 'boolean') return String(value);
      return cannot(JSON.stringify(value));
    case 'Array':
      return Array.isArray(value) ? value : cannot(JSON.stringify(value));
    default:
      return isObject(value) ? value : cannot(JSON.stringify(value));
  }
}

/** ref: store.rs:587-597 infer_base_type */
export function inferBaseType(value) {
  if (typeof value === 'number') return 'Number';
  if (typeof value === 'boolean') return 'Boolean';
  if (typeof value === 'string') return 'Text';
  if (Array.isArray(value)) return 'Array';
  if (isObject(value)) return 'Object';
  return null;
}

/** Asset im Realm laden; sonst „asset not found“. ref: commands.rs:577-589 validate_asset_realm */
export async function loadAssetInRealm(ctx, assetId, realm) {
  const row = await ctx.get(C.assets, assetId);
  if (!row || row.realm !== realm || !isObject(row.data)) fail('not_found', `asset not found: ${assetId}`);
  return row.data;
}

// ---------------------------------------------------------------- Alarme

// ref: alarms.rs:29-75 Severity/Status/can_transition_to
const STATUS_WIRE = { Open: 'OPEN', Acknowledged: 'ACKNOWLEDGED', InProgress: 'IN_PROGRESS', Resolved: 'RESOLVED', Closed: 'CLOSED' };
const NEXT = {
  Open: ['Acknowledged', 'InProgress', 'Resolved', 'Closed'],
  Acknowledged: ['InProgress', 'Resolved', 'Closed'],
  InProgress: ['Resolved', 'Closed'],
  Resolved: ['Open', 'Closed'],
  Closed: [],
};
export const canTransition = (from, to) => from === to || (NEXT[from] ?? []).includes(to);
export const statusWire = (status) => STATUS_WIRE[status] ?? String(status);

/** ref: commands.rs:528-537 parse_status */
export function parseStatus(name) {
  switch (name.toUpperCase()) {
    case 'OPEN': return 'Open';
    case 'ACKNOWLEDGED': case 'ACK': return 'Acknowledged';
    case 'IN_PROGRESS': case 'INPROGRESS': case 'PROGRESS': return 'InProgress';
    case 'RESOLVED': return 'Resolved';
    case 'CLOSED': return 'Closed';
    default: return fail('invalid', `unknown alarm status: ${name.toUpperCase()}`);
  }
}

/** ref: commands.rs:480-505 alarm_row */
export const alarmRow = (alarm) => ({
  realm: alarm.realm, title: alarm.title, severity: String(alarm.severity).toUpperCase(), status: statusWire(alarm.status),
  assignee_id: alarm.assignee_id, source: alarm.source, created_ms: alarm.created, data: alarm,
  index_text: alarm.title.toLowerCase(), sort_key: zeroPad(alarm.created, 20), status_key: statusWire(alarm.status),
});

/** ref: conditions.rs:923-930 severity_from */
export function severityFrom(label) {
  const upper = typeof label === 'string' ? label.toUpperCase() : null;
  return upper === 'LOW' ? 'Low' : upper === 'HIGH' ? 'High' : 'Medium';
}

/** ref: alarms.rs:211 create (Status Open, created = last_modified = now) */
export async function createAlarm(ctx, { realm, title, content, severity, source, sourceId, assetIds }) {
  const now = ctx.now();
  const alarm = {
    id: generateId(ctx), realm, title, content: content ?? null, severity, status: 'Open', assignee_id: null,
    source, source_id: sourceId, created: now, last_modified: now, asset_ids: assetIds ?? [],
  };
  await ctx.put(C.alarms, compact({ ...alarmRow(alarm), id: alarm.id, created_at_ms: alarm.created, updated_at_ms: now }));
  return alarm;
}

// ---------------------------------------------------------------- Rulesets/Dashboards/Widgets/Agents (Zeilen)

/** ref: commands.rs:1207-1240 project_ruleset */
export const rulesetRow = (r) => ({
  realm: r.realm, name: r.name, enabled: r.enabled, data: r.data, last_fired_ms: r.last_fired_ms, match_state: r.match_state,
  index_text: r.name.toLowerCase(), sort_key: r.name, status_key: r.enabled ? 'enabled' : 'disabled',
});

/** ref: commands.rs:1242-1260 project_dashboard */
export const dashboardRow = (d) => ({
  realm: d.realm, name: d.name, scope: d.scope, scope_ref: d.scope_ref, view_mode: d.view_mode, sort_index: d.sort_index,
  index_text: d.name.toLowerCase(), sort_key: d.name,
});

/** ref: commands.rs:1360-1399 project_widget */
export const widgetRow = (w) => ({
  dashboard_id: w.dashboard_id, realm: w.realm, signal_ref: w.signal_ref, cond_text: w.cond_text, action_prompt: w.action_prompt,
  trigger_status: w.trigger_status, trigger_state: w.trigger_state, trigger_code: w.trigger_code, render_code: w.render_code,
  x: w.x, y: w.y, w: w.w, h: w.h, sort_index: w.sort_index,
  index_text: (w.cond_text ?? '').toLowerCase(), sort_key: zeroPad(w.sort_index ?? 0, 8), status_key: w.trigger_status ?? 'idle',
});

/** ref: commands.rs:1900-1925 project_agent */
export const agentRow = (a) => ({
  realm: a.realm, name: a.name, kind: a.kind, enabled: a.enabled, data: a.data,
  index_text: a.name.toLowerCase(), sort_key: a.name, status_key: a.kind,
});

/** ref: commands.rs:1927-1964 project_agent_status */
export const agentStatusRow = (s) => ({
  realm: s.realm, agent_id: s.agent_id, link_state: s.link_state, last_event_ms: s.last_event_ms, error: s.error,
  data: {}, status_key: s.link_state,
});

/** Widget laden, Realm prüfen; sonst „widget not found“. ref: commands.rs:1616-1642, 1536-1546 */
export async function loadWidgetInRealm(ctx, widgetId, realm) {
  const row = await ctx.get(C.widgets, widgetId);
  if (!row || row.realm !== realm) fail('not_found', `widget not found: ${widgetId}`);
  return row;
}

// ---------------------------------------------------------------- Wächter (Widget-Trigger)
//
// Der Rust-Wächter war ein Rhai-Programm (watcher.rs) – eine Skript-Engine im Core. In der App ist der Wächter
// eine deklarative JSON-Spezifikation, die der Handler selbst prüft und auswertet (kein eval, kein Core-Code):
//   { "when": <expr>, "reason": "Text", "for_ms"?: n }
//   expr  = {op: gt|gte|lt|lte|eq|neq, left: val, right: val} | {op: and|or, items: [expr]} | {op: not, item: expr}
//   val   = Zahl | {fn: last|avg|min|max|count|rate|age_ms|has_data, window?: "15m"}
// for_ms = „seit X“: feuert erst, wenn die Bedingung for_ms lang ununterbrochen gilt (Zustand in trigger_state).
// Deckel wie watcher.rs:32-37: Quelltext ≤ 64 KiB, Schachtelung ≤ 32.

const MAX_SCRIPT_BYTES = 64 * 1024; // ref: watcher.rs:37
const MAX_CALL_LEVELS = 32; // ref: watcher.rs:33
const CMP = ['gt', 'gte', 'lt', 'lte', 'eq', 'neq'];
const FNS = ['last', 'avg', 'min', 'max', 'count', 'rate', 'age_ms', 'has_data'];

/** ref: watcher.rs parse_duration (ms/s/m/h/d) */
export function parseDuration(text) {
  const m = /^(\d+)(ms|s|m|h|d)$/.exec(typeof text === 'string' ? text.trim() : '');
  if (!m) throw new Error(`invalid duration: ${text}`);
  const unit = { ms: 1, s: 1000, m: 60_000, h: 3_600_000, d: 86_400_000 }[m[2]];
  return Number(m[1]) * unit;
}

function checkValue(v, depth) {
  if (typeof v === 'number' && Number.isFinite(v)) return;
  if (!isObject(v) || !FNS.includes(v.fn)) throw new Error('value must be a number or {fn}');
  if (v.window !== undefined) parseDuration(v.window);
  if (depth > MAX_CALL_LEVELS) throw new Error('nesting too deep');
}

function checkExpr(e, depth) {
  if (depth > MAX_CALL_LEVELS) throw new Error('nesting too deep');
  if (!isObject(e)) throw new Error('expression must be an object');
  if (CMP.includes(e.op)) { checkValue(e.left, depth + 1); checkValue(e.right, depth + 1); return; }
  if (e.op === 'and' || e.op === 'or') {
    if (!Array.isArray(e.items) || e.items.length === 0) throw new Error(`${e.op} needs items`);
    for (const item of e.items) checkExpr(item, depth + 1);
    return;
  }
  if (e.op === 'not') { checkExpr(e.item, depth + 1); return; }
  throw new Error(`unknown op: ${e.op}`);
}

/** Wächter prüfen: null = lauffähig, sonst Fehlertext. ref: watcher.rs:384-391 validate_program (Leerprogramm = Fehler) */
export function validateWatcher(code) {
  if (typeof code !== 'string' || code.trim() === '') return 'empty watcher program';
  if (new TextEncoder().encode(code).length > MAX_SCRIPT_BYTES) return 'watcher program too large';
  let spec;
  try { spec = JSON.parse(code); } catch (error) { return `watcher is not valid JSON: ${error.message}`; }
  try {
    if (!isObject(spec)) throw new Error('watcher must be an object');
    checkExpr(spec.when, 1);
    if (spec.reason !== undefined && typeof spec.reason !== 'string') throw new Error('reason must be a string');
    if (spec.for_ms !== undefined && !(Number.isInteger(spec.for_ms) && spec.for_ms >= 0)) throw new Error('for_ms must be a non-negative integer');
  } catch (error) {
    return error.message;
  }
  return null;
}

/** Status nach Upsert/Fortsetzen. ref: commands.rs:1440-1452, 1548-1559 */
export function statusForCode(code) {
  if (typeof code !== 'string' || code.trim() === '') return 'idle';
  return validateWatcher(code) === null ? 'armed' : 'needs_attention';
}

function seriesValue(v, points, nowMs) {
  if (typeof v === 'number') return v;
  const from = v.window ? nowMs - parseDuration(v.window) : -Infinity;
  const pts = points.filter(([t]) => t >= from);
  const vals = pts.map(([, value]) => value);
  switch (v.fn) {
    case 'last': return points.length ? points[points.length - 1][1] : NaN;
    case 'has_data': return points.length ? 1 : 0;
    case 'age_ms': return points.length ? nowMs - points[points.length - 1][0] : Infinity;
    case 'count': return vals.length;
    case 'avg': return vals.length ? vals.reduce((a, b) => a + b, 0) / vals.length : NaN;
    case 'min': return vals.length ? Math.min(...vals) : NaN;
    case 'max': return vals.length ? Math.max(...vals) : NaN;
    case 'rate': {
      if (pts.length < 2) return 0;
      const [t0, v0] = pts[0];
      const [t1, v1] = pts[pts.length - 1];
      return t1 === t0 ? 0 : (v1 - v0) / ((t1 - t0) / 1000);
    }
    default: return NaN;
  }
}

function evalExpr(e, points, nowMs) {
  if (e.op === 'and') return e.items.every((item) => evalExpr(item, points, nowMs));
  if (e.op === 'or') return e.items.some((item) => evalExpr(item, points, nowMs));
  if (e.op === 'not') return !evalExpr(e.item, points, nowMs);
  const l = seriesValue(e.left, points, nowMs);
  const r = seriesValue(e.right, points, nowMs);
  if (Number.isNaN(l) || Number.isNaN(r)) return false;
  return { gt: l > r, gte: l >= r, lt: l < r, lte: l <= r, eq: l === r, neq: l !== r }[e.op];
}

/** Wächter auswerten: {fired[], state, error?}. ref: watcher.rs:297 evaluate (zustandsbehaftet je Datenpunkt) */
export function evaluateWatcher(code, points, nowMs, stateIn) {
  const error = validateWatcher(code);
  const state = isObject(stateIn) ? { ...stateIn } : {};
  if (error) return { fired: [], state, error };
  const spec = JSON.parse(code);
  const holds = evalExpr(spec.when, points, nowMs);
  if (!holds) { delete state.since_ms; return { fired: [], state }; }
  if (state.since_ms === undefined) state.since_ms = nowMs;
  const forMs = spec.for_ms ?? 0;
  if (nowMs - state.since_ms < forMs) return { fired: [], state };
  return { fired: [spec.reason ?? 'Bedingung erfüllt'], state };
}

// ---------------------------------------------------------------- Regeln (Rulesets)

/** ref: conditions.rs:443-476 eval_value_predicate */
export function evalValuePredicate(p, v) {
  const numeric = typeof v === 'number' ? v : typeof v === 'boolean' ? (v ? 1 : 0) : null;
  const num = (f) => (numeric === null ? false : f(numeric));
  switch (p?.op) {
    case 'is_null': return v === null || v === undefined;
    case 'not_null': return !(v === null || v === undefined);
    case 'eq': return JSON.stringify(v ?? null) === JSON.stringify(p.value ?? null);
    case 'neq': return JSON.stringify(v ?? null) !== JSON.stringify(p.value ?? null);
    case 'greater_than': return num((n) => n > p.value);
    case 'greater_equal': return num((n) => n >= p.value);
    case 'less_than': return num((n) => n < p.value);
    case 'less_equal': return num((n) => n <= p.value);
    case 'between': return num((n) => n >= p.min && n <= p.max);
    case 'contains': return typeof v === 'string' && v.includes(p.value);
    case 'regex':
      if (typeof v !== 'string') return false;
      try { return new RegExp(p.pattern).test(v); } catch { return false; }
    default: return false;
  }
}


// ---------------------------------------------------------------- Bedingungen (§2A.16/18/23)

/** Ergebnis einer Bedingung oder Gruppe. ref: conditions.rs:565-578 CondResult */
export const COND = Object.freeze({ MATCHED: 'matched', UNMATCHED: 'unmatched', PENDING: 'pending', NA: 'na' });

const isOr = (operator) => operator === 'Or' || operator === 'OR' || operator === 'or';
const isMillis = (v) => v === undefined || v === null || Number.isInteger(v);

/**
 * Formprüfung wie serde `JsonRule`: `duration`/`timestamp_older_than` sind DurationSpec (i64-Millisekunden, transparent),
 * `meta` ist Vec<MetaPredicate{name, value?}>, `attribute.name` Pflicht, `track_unmatched` bool. Fehlerhafte Regel wird bei
 * der Auswertung übersprungen und stört Geschwister-Regeln nicht.
 * ref: conditions.rs:206-210 parse_rule, :726-733 (malformed → continue), :84-128 (Felder)
 */
export function ruleIsWellFormed(rule) {
  if (!isObject(rule)) return false;
  if (rule.track_unmatched != null && typeof rule.track_unmatched !== 'boolean') return false;
  const metaOk = (m) => isObject(m) && typeof m.name === 'string' && (m.value == null || isObject(m.value));
  const condOk = (c) => isObject(c) && isObject(c.attribute) && typeof c.attribute.name === 'string'
    && isMillis(c.duration) && isMillis(c.attribute.timestamp_older_than)
    && (c.attribute.meta == null || (Array.isArray(c.attribute.meta) && c.attribute.meta.every(metaOk)));
  const groupOk = (g) => g == null || (isObject(g)
    && (g.items == null || (Array.isArray(g.items) && g.items.every(condOk)))
    && (g.groups == null || (Array.isArray(g.groups) && g.groups.every(groupOk))));
  return groupOk(rule.when);
}

/**
 * Ein AttributePredicate gegen Wert/Zeitstempel/Meta des Attributs und den Vorwert des Ereignisses.
 * ref: conditions.rs:474-542 as_attribute_matcher (AssetQueryPredicate.java:189-302)
 */
export function attributeMatches(pred, value, timestampMs, meta, oldValue, nowMs) {
  // ref: conditions.rs:492-499 – timestampOlderThan: nicht erfüllt, solange attr.timestamp > now - dur
  if (pred.timestamp_older_than != null && timestampMs > nowMs - pred.timestamp_older_than) return false;
  // ref: conditions.rs:501-508 – Wertprädikat, fehlender Wert = null
  if (pred.value != null && !evalValuePredicate(pred.value, value ?? null)) return false;
  // ref: conditions.rs:509-524 – meta: UND über alle Meta-Prädikate, je anyMatch über die MetaMap des Attributs
  const metaMap = isObject(meta) ? meta : {};
  for (const mp of pred.meta ?? []) {
    const any = Object.keys(metaMap).some((k) => k === mp.name && (mp.value == null || evalValuePredicate(mp.value, metaMap[k])));
    if (!any) return false;
  }
  // ref: conditions.rs:526-533 – previousValue gegen event.old_value (fehlend = null), nicht gegen den neuen Wert
  if (pred.previous_value != null && !evalValuePredicate(pred.previous_value, oldValue ?? null)) return false;
  return true;
}

/** ref: conditions.rs:544-563 asset_query_matches */
function assetQueryMatches(cond, asset) {
  if ((cond.asset_types ?? []).length && !cond.asset_types.includes(asset.asset_type)) return false;
  if (cond.realm != null && cond.realm !== asset.realm) return false;
  if ((cond.asset_ids ?? []).length && !cond.asset_ids.includes(asset.id)) return false;
  return true;
}

/**
 * Eine Bedingung gegen den Live-Zustand des Assets, mit Dauerfenster (§2A.16). `windows` = {pfad: first_match_ms} des
 * Assets (iot_rulesets.match_state[asset].windows; Rust: iot_condition_state.first_match_ms je (ruleset, index, asset)).
 * Fehlendes Attribut: Wert null, Zeitstempel 0, leere Meta (wie Rust). previous_value nur für das Ereignisattribut.
 * ref: conditions.rs:580-666 evaluate_condition
 */
export function evaluateCondition(cond, path, asset, event, nowMs, windows) {
  if (!assetQueryMatches(cond, asset)) return COND.NA;
  const name = cond.attribute.name;
  const target = isObject(asset.attributes?.[name]) ? asset.attributes[name] : null;
  const isEvent = name === event.attribute_name;
  const value = target ? target.value ?? null : isEvent ? event.value ?? null : null;
  const timestampMs = Number.isInteger(target?.timestamp) ? target.timestamp : 0;
  const oldValue = isEvent ? event.old_value ?? null : null; // ref: conditions.rs:608-613
  const holds = attributeMatches(cond.attribute, value, timestampMs, target?.meta ?? {}, oldValue, nowMs);
  // ref: conditions.rs:625-631 – ohne duration sofort Matched/Unmatched
  if (cond.duration == null) return holds ? COND.MATCHED : COND.UNMATCHED;
  if (holds) {
    const first = windows[path];
    // ref: conditions.rs:635-643 – duration_matches = first_match_ms + dur <= now
    if (Number.isInteger(first)) return first + cond.duration <= nowMs ? COND.MATCHED : COND.PENDING;
    windows[path] = nowMs; // ref: conditions.rs:644-649 – Fenster startet mit dieser Auswertung
    return COND.PENDING;
  }
  // ref: conditions.rs:651-659 – Prädikat falsch → first_match_ms löschen; nächstes true startet neu (kein Fortsetzen)
  delete windows[path];
  return COND.UNMATCHED;
}

/**
 * AND/OR-Gruppe: nur anwendbare Bedingungen zählen (NotApplicable fällt heraus, keine anwendbare → NA).
 * AND = alle Matched, OR = eine Matched (ref: conditions.rs:739-761). Alle Bedingungen werden ausgewertet (kein
 * Kurzschluss), damit jedes Dauerfenster fortgeschrieben wird. Verschachtelte Gruppen (App-Erweiterung; Rust wertet nur
 * die Wurzel-items aus) liefern dasselbe Ergebnis rekursiv. `primary` = Attribut der ersten Matched-Bedingung
 * (ref: conditions.rs:782-795 primary_attr).
 */
export function evaluateGroup(group, prefix, asset, event, nowMs, windows) {
  const g = isObject(group) ? group : {};
  const items = (g.items ?? []).map((c, i) => ({ res: evaluateCondition(c, `${prefix}${i}`, asset, event, nowMs, windows), attr: c.attribute.name }));
  const subs = (g.groups ?? []).map((sub, i) => evaluateGroup(sub, `${prefix}g${i}.`, asset, event, nowMs, windows));
  const applicable = [...items.map((x) => x.res), ...subs.map((s) => s.result)].filter((r) => r !== COND.NA);
  let result;
  if (!applicable.length) result = COND.NA;
  else if (isOr(g.operator)) result = applicable.includes(COND.MATCHED) ? COND.MATCHED : applicable.includes(COND.PENDING) ? COND.PENDING : COND.UNMATCHED;
  else result = applicable.every((r) => r === COND.MATCHED) ? COND.MATCHED : applicable.includes(COND.UNMATCHED) ? COND.UNMATCHED : COND.PENDING;
  const primary = items.find((x) => x.res === COND.MATCHED)?.attr ?? subs.find((s) => s.primary)?.primary ?? null;
  return { result, applicable, primary };
}

/** Betrifft die Regel das Ereignisattribut überhaupt? (Ereignisgetriebene Auswertung, conditions.rs:702) */
export function ruleMentions(group, attributeName) {
  const g = isObject(group) ? group : {};
  return (g.items ?? []).some((c) => c?.attribute?.name === attributeName)
    || (g.groups ?? []).some((sub) => ruleMentions(sub, attributeName));
}

/** Enthält die Gruppe eine Bedingung, deren Feld `pick(cond)` gesetzt ist? */
function groupHas(group, pick) {
  const g = isObject(group) ? group : {};
  return (g.items ?? []).some((c) => isObject(c) && pick(c) != null) || (g.groups ?? []).some((sub) => groupHas(sub, pick));
}
/** Zeitbasiert = Dauerfenster oder timestamp_older_than: Ergebnis kann sich ohne neues Signal ändern (every-Trigger). */
export const ruleIsTimeBased = (group) => groupHas(group, (c) => c.duration) || ruleUsesOlderThan(group);
export const ruleUsesOlderThan = (group) => groupHas(group, (c) => c.attribute?.timestamp_older_than);

/** ref: conditions.rs:280-300 recurrence_scope_key */
export const recurrenceKey = (assetId, recurrence) => (recurrence?.scope === 'GLOBAL' ? '*' : assetId);

/**
 * Eine Regel gegen ein Asset auswerten und Treffer ausgeben (Alarm, optional agent.task). Gemeinsam für den on-Trigger
 * (ctox.iot.signal.evaluate, Ereignis = geändertes Attribut) und den every-Trigger (ctox.iot.rules.tick, Ereignis ohne
 * Attribut: Ablauf eines Dauerfensters / timestamp_older_than ohne neues Signal).
 * Zustand in iot_rulesets.match_state: [recurrenceKey] = {matched, fired_ms}; [asset.id] zusätzlich
 * {windows: {pfad: first_match_ms}, unmatched: true} (Rust: iot_condition_state first_match_ms/previously_unmatched).
 * Rückgabe: geschriebene Regelzeile oder null (übersprungen, nichts geschrieben).
 * ref: conditions.rs:702-920 evaluate_event, :944-1115 emit_matches
 */
export async function evaluateRuleset(ctx, ruleset, asset, event, nowMs, report) {
  const rule = isObject(ruleset.data) ? ruleset.data : {};
  if (!ruleIsWellFormed(rule)) return null; // ref: conditions.rs:726-733
  // Zeitbasierte Regeln werten (wie Rust) bei jedem Ereignis des Assets aus, sonst nur bei erwähntem Attribut.
  if (event.attribute_name != null && !ruleIsTimeBased(rule.when) && !ruleMentions(rule.when, event.attribute_name)) return null;
  const matchState = { ...(isObject(ruleset.match_state) ? ruleset.match_state : {}) };
  const own = isObject(matchState[asset.id]) ? matchState[asset.id] : {};
  const windows = { ...(isObject(own.windows) ? own.windows : {}) };
  const { result, applicable, primary } = evaluateGroup(rule.when, '', asset, event, nowMs, windows);
  if (result === COND.NA) return null; // ref: conditions.rs:736-751 – leer/keine anwendbare Bedingung: kein Zustand
  const matched = result === COND.MATCHED;
  const recurrence = isObject(rule.recurrence) ? rule.recurrence : {};
  const key = recurrenceKey(asset.id, recurrence);
  const before = isObject(matchState[key]) ? matchState[key] : { matched: false };
  let fire = false;
  if (matched) {
    // recurrence.mins: null → nur beim Übergang nicht-erfüllt → erfüllt; 0 → jedes Mal; >0 → Sperrfenster (conditions.rs:302-342)
    const mins = Number.isInteger(recurrence.mins) ? recurrence.mins : null;
    if (mins === null) fire = !before.matched;
    else if (mins === 0) fire = true;
    else fire = before.fired_ms === undefined || nowMs >= before.fired_ms + mins * 60_000;
  }
  // §2A.18 track_unmatched / otherwise: nur im AND-Modus, nur wenn eine anwendbare Bedingung Unmatched ist (nicht
  // Pending), nur das NEU-Unmatched passiert den previously_unmatched-Filter; Wiedertreffer setzt den Filter zurück.
  // ref: conditions.rs:856-890, :892-903
  let unmatched = own.unmatched === true;
  let otherwise = false;
  if (matched) unmatched = false;
  else if (rule.track_unmatched === true && !isOr(rule.when?.operator) && applicable.includes(COND.UNMATCHED) && !unmatched) {
    unmatched = true;
    otherwise = true;
  }
  matchState[key] = { matched, ...(fire ? { fired_ms: nowMs } : before.fired_ms !== undefined ? { fired_ms: before.fired_ms } : {}) };
  const assetEntry = { ...(isObject(matchState[asset.id]) ? matchState[asset.id] : {}) };
  delete assetEntry.windows;
  delete assetEntry.unmatched;
  if (Object.keys(windows).length) assetEntry.windows = windows;
  if (unmatched) assetEntry.unmatched = true;
  if (Object.keys(assetEntry).length) matchState[asset.id] = assetEntry;
  else delete matchState[asset.id];
  const onMatch = isObject(rule.on_match) ? rule.on_match : {};
  const attribute = primary ?? event.attribute_name;
  if (fire && onMatch.raise_alarm !== false) {
    const alarm = await createAlarm(ctx, {
      realm: asset.realm, title: `IoT: ${rule.name ?? ruleset.name} on ${asset.id}`,
      content: `ruleset=${ruleset.name} attribute=${attribute} matched`, severity: severityFrom(onMatch.alarm_severity),
      source: asset.realm.trim() === '' ? 'GlobalRuleset' : 'RealmRuleset', sourceId: ruleset.id, assetIds: [asset.id],
    });
    report.alarms.push(alarm.id);
    // ref: conditions.rs:1025-1066 (Queue-Task je Treffer). §16.30: Skill aus dem eigenen Paket (skills/operations);
    // `on_match.suggested_skill` ist Collection-Datum und wird nicht mehr als Skill-Name übernommen (fremder
    // Namensraum/Core-Skill wäre unzulässig). Ergebnis kommt als Rückruf ctox.iot.task.complete/.failed an den Alarm.
    if (onMatch.enqueue_task) {
      await ctx.cap.agent.task.enqueue({
        title: `IoT: ${rule.name ?? ruleset.name} on ${asset.id}`, instruction: onMatch.task_prompt ?? `Alarm ${alarm.id}: ${alarm.content}`,
        skill: 'iot/operations', thread_key: `iot-ruleset:${ruleset.id}:${asset.id}`,
        payload: { alarm_id: alarm.id, ruleset_id: ruleset.id, asset_id: asset.id },
        idempotency_key: `iot-alarm:${alarm.id}`, on_complete: 'ctox.iot.task.complete', on_failed: 'ctox.iot.task.failed',
      });
    }
  }
  // OtherwiseUnmatched löst (wie emit_matches) keinen Alarm und keine Arbeit aus; ref: conditions.rs:964-968
  const row = { ...ruleset, match_state: matchState, last_fired_ms: fire ? nowMs : ruleset.last_fired_ms };
  await writeRow(ctx, C.rulesets, ruleset.id, rulesetRow(row), ruleset);
  if (fire) report.rulesets.push(ruleset.id);
  if (otherwise) report.otherwise.push(ruleset.id);
  return row;
}
