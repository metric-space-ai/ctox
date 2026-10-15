// collections.schema.json and schema diff (§16.22 A2/A6/A9, §16.4). Same rules as the binding stub
// (~/.local/state/ctox-sync-v4/stub, ctox-app-api-stub.mjs, block "collections.schema.json und Schema-Diff"),
// copied verbatim so stub and kernel cannot classify a schema change differently.
import { CtoxError, cloneValue, isPlainObject } from './values.mjs';
export const SCHEMA_FILE = 'collections.schema.json';
const SCHEMA_META_KEYS = ['class', 'rights', 'client_limit', 'byte_budget'];
function canon(v) {
  if (Array.isArray(v)) return `[${v.map(canon).join(',')}]`;
  if (v && typeof v === 'object') return `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canon(v[k])}`).join(',')}}`;
  return JSON.stringify(v ?? null);
}
const sameCanon = (a, b) => canon(a) === canon(b);
function normalizeSchemaEntry(entry) {
  const e = isPlainObject(entry) ? entry : {};
  const wrapped = isPlainObject(e.schema);
  const schema = { ...(wrapped ? e.schema : e) };
  if (!wrapped) for (const k of SCHEMA_META_KEYS) delete schema[k];
  const indexes = (wrapped ? e.indexes : undefined) ?? schema.indexes ?? [];
  delete schema.indexes;
  const out = { schema, indexes };
  for (const k of SCHEMA_META_KEYS) out[k] = e[k] ?? null;
  return out;
}
/** Text oder Objekt → { collections: {name: {schema, indexes, class, rights, client_limit, byte_budget}}, migrations }. */
export function parseCollectionsFile(input) {
  let obj;
  try { obj = typeof input === 'string' ? (input.trim() ? JSON.parse(input) : {}) : (input ?? {}); } catch (e) {
    throw new CtoxError('invalid', `${SCHEMA_FILE}: kein JSON (${e.message})`, { path: SCHEMA_FILE });
  }
  if (!isPlainObject(obj)) throw new CtoxError('invalid', `${SCHEMA_FILE}: Objekt erwartet`, { path: SCHEMA_FILE });
  if (obj.collections !== undefined && !isPlainObject(obj.collections)) throw new CtoxError('invalid', `${SCHEMA_FILE}: collections muss ein Objekt sein`, { path: `${SCHEMA_FILE}.collections` });
  if (obj.migrations !== undefined && !isPlainObject(obj.migrations)) throw new CtoxError('invalid', `${SCHEMA_FILE}: migrations muss ein Objekt sein`, { path: `${SCHEMA_FILE}.migrations` });
  const collections = {};
  for (const [name, entry] of Object.entries(obj.collections ?? {})) collections[name] = normalizeSchemaEntry(entry);
  return { collections, migrations: cloneValue(obj.migrations ?? {}) };
}
function manifestCollectionsToParsed(mc) {
  const collections = {};
  for (const [name, c] of Object.entries(mc ?? {})) collections[name] = normalizeSchemaEntry(c);
  return { collections, migrations: {} };
}
function parsedToManifestCollections(parsed) {
  const out = {};
  for (const [name, c] of Object.entries(parsed.collections)) {
    out[name] = { schema: { ...c.schema, ...(c.indexes?.length ? { indexes: c.indexes } : {}) }, class: c.class, rights: c.rights, client_limit: c.client_limit, byte_budget: c.byte_budget };
  }
  return out;
}
const typeLabel = (p) => (!p || typeof p !== 'object' ? '?' : Array.isArray(p.type) ? p.type.join('|') : (p.type ?? (p.enum ? 'enum' : 'any')));
function diffCollection(a, b) {
  const compatible = [], generation = [];
  const pa = a.schema.properties ?? {}, pb = b.schema.properties ?? {};
  const ra = new Set(a.schema.required ?? []), rb = new Set(b.schema.required ?? []);
  for (const [field, def] of Object.entries(pb)) {
    if (!(field in pa)) {
      const optional = !rb.has(field);
      const hasDefault = isPlainObject(def) && 'default' in def;
      if (optional && hasDefault) compatible.push(`Feld „${field}“ neu (optional, Default)`);
      else generation.push(`Feld „${field}“ neu ${optional ? 'ohne Default' : 'als Pflichtfeld'}`);
    } else if (!sameCanon(pa[field], def)) {
      generation.push(typeLabel(pa[field]) !== typeLabel(def) ? `Typ von „${field}“: ${typeLabel(pa[field])} → ${typeLabel(def)}` : `Feld „${field}“ geändert`);
    }
  }
  for (const field of Object.keys(pa)) if (!(field in pb)) generation.push(`Feld „${field}“ entfernt`);
  for (const field of rb) if (!ra.has(field) && field in pa) generation.push(`„${field}“ wird Pflichtfeld`);
  for (const field of ra) if (!rb.has(field) && field in pb) generation.push(`„${field}“ nicht mehr Pflicht`);
  for (const k of new Set([...Object.keys(a.schema), ...Object.keys(b.schema)])) {
    if (k !== 'properties' && k !== 'required' && !sameCanon(a.schema[k], b.schema[k])) generation.push(`Schema-Eigenschaft „${k}“ geändert`);
  }
  if (!sameCanon(a.indexes, b.indexes)) compatible.push('Indizes geändert');
  for (const k of SCHEMA_META_KEYS) if (!sameCanon(a[k], b[k])) compatible.push(k === 'rights' ? 'Rechte geändert' : `${k} geändert`);
  return { compatible, generation };
}
/**
 * Je Collection { name, kind: 'unchanged'|'compatible'|'new'|'generation'|'removed', reasons[], needsMigration, migration }.
 * Eine Migration zählt nur mit from_ver === baseVer (sonst Rest einer früheren Generation) und migrate_fn oder steps.
 */
export function classifyCollections(baseParsed, nextParsed, { baseVer = null } = {}) {
  const a = baseParsed?.collections ?? {}, b = nextParsed?.collections ?? {};
  return [...new Set([...Object.keys(a), ...Object.keys(b)])].sort().map((name) => {
    if (!(name in a)) return { name, kind: 'new', reasons: ['Collection neu'], needsMigration: false, migration: null };
    if (!(name in b)) return { name, kind: 'removed', reasons: ['Collection entfernt (wird archiviert)'], needsMigration: false, migration: null };
    const { compatible, generation } = diffCollection(a[name], b[name]);
    const kind = generation.length ? 'generation' : compatible.length ? 'compatible' : 'unchanged';
    const m = nextParsed.migrations?.[name] ?? null;
    const usable = m && (m.migrate_fn || Array.isArray(m.steps)) && (baseVer == null || String(m.from_ver) === String(baseVer));
    return { name, kind, reasons: [...generation, ...compatible], needsMigration: kind === 'generation', migration: kind === 'generation' && usable ? cloneValue(m) : null };
  });
}
// Deklarative Migrationsschritte (Stub-Teilmenge): rename{from,to} · add{field,value} · remove{field} · cast{field,to}
export function applyMigrationSteps(doc, steps) {
  const d = { ...doc };
  for (const st of steps ?? []) {
    switch (st?.op) {
      case 'rename': if (st.from in d) { d[st.to] = d[st.from]; delete d[st.from]; } break;
      case 'add': if (!(st.field in d)) d[st.field] = cloneValue(st.value ?? null); break;
      case 'remove': delete d[st.field]; break;
      case 'cast': if (st.field in d && d[st.field] != null) {
        d[st.field] = st.to === 'string' ? String(d[st.field]) : st.to === 'number' ? Number(d[st.field]) : st.to === 'boolean' ? Boolean(d[st.field]) : d[st.field];
      } break;
      default: throw new CtoxError('invalid', `Unbekannter Migrationsschritt ${st?.op}`, { path: 'steps' });
    }
  }
  return d;
}
export { manifestCollectionsToParsed, parsedToManifestCollections };
