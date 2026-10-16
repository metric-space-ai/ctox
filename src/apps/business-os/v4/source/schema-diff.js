// Shell v4 – Quelltext-Modus: Schema-Diff für collections.schema.json (Architektur §9.1, §16.4).
//
// Reine Funktionen (Browser und Node). Die Shell zeigt damit je Collection „kompatibel“ oder „neue Generation“ an
// und verlangt bei inkompatiblen Änderungen eine Migration. Entscheidend ist die Prüfung im Core (§9.2 Schritt 3);
// die Shell spiegelt sie nur, damit „Veröffentlichen“ nicht erst am Core scheitert.
//
// Regeln (Owner 15.10. + §16.4, streng):
//   kompatibel     = neue Collection, neues OPTIONALES Feld MIT Default, Index, Rechte, Klasse/Limit/Budget
//   neue Generation = alles andere (Typänderung, Umbenennen/Entfernen eines Felds, neues Pflichtfeld, neues Feld ohne
//                    Default, geänderte Pflichtliste, sonstige Schema-Eigenschaften) → Pflicht-Migration
//   entfernt       = Collection fehlt im Ziel (wird archiviert, §9.5); keine Migration verlangt
//
// Dateiformat collections.schema.json (beide Formen werden gelesen):
//   { "collections": { "<name>": <JSON-Schema> | { "schema": <JSON-Schema>, "class", "rights", "client_limit",
//     "byte_budget", "indexes" } }, "migrations": { "<name>": { "from_ver": "<base_app_ver>", "migrate_fn": "<pfad>" } | { "from_ver", "steps": [...] } } }
// Indizes dürfen auch als schema.indexes stehen (RxDB-Form).

const META_KEYS = ['class', 'rights', 'client_limit', 'byte_budget'];

function canon(v) {
  if (Array.isArray(v)) return `[${v.map(canon).join(',')}]`;
  if (v && typeof v === 'object') return `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canon(v[k])}`).join(',')}}`;
  return JSON.stringify(v ?? null);
}
const same = (a, b) => canon(a) === canon(b);

function normalizeEntry(entry) {
  const e = entry && typeof entry === 'object' ? entry : {};
  const isWrapped = e.schema && typeof e.schema === 'object';
  const schema = { ...(isWrapped ? e.schema : e) };
  if (!isWrapped) for (const k of META_KEYS) delete schema[k];
  const indexes = (isWrapped ? e.indexes : undefined) ?? schema.indexes ?? [];
  delete schema.indexes;
  const out = { schema, indexes };
  for (const k of META_KEYS) out[k] = (isWrapped ? e[k] : entry?.[k]) ?? null;
  return out;
}

/** Text oder Objekt → { collections: {name: {schema, indexes, class, rights, client_limit, byte_budget}}, migrations } */
export function parseCollectionsFile(input) {
  const obj = typeof input === 'string' ? (input.trim() ? JSON.parse(input) : {}) : (input ?? {});
  if (!obj || typeof obj !== 'object' || Array.isArray(obj)) throw new Error('collections.schema.json: Objekt erwartet');
  const collections = {};
  for (const [name, entry] of Object.entries(obj.collections ?? {})) collections[name] = normalizeEntry(entry);
  return { collections, migrations: { ...(obj.migrations ?? {}) } };
}

/** Manifest-Form §9.1 (für den Core / Stub: manifest.collections). */
export function toManifestCollections(parsed) {
  const out = {};
  for (const [name, c] of Object.entries(parsed.collections)) {
    out[name] = { schema: { ...c.schema, ...(c.indexes?.length ? { indexes: c.indexes } : {}) }, class: c.class, rights: c.rights, client_limit: c.client_limit, byte_budget: c.byte_budget };
  }
  return out;
}

function typeLabel(p) {
  if (!p || typeof p !== 'object') return '?';
  return Array.isArray(p.type) ? p.type.join('|') : (p.type ?? (p.enum ? 'enum' : 'any'));
}

function diffCollection(a, b) {
  const compatible = [];
  const generation = [];
  const pa = a.schema.properties ?? {};
  const pb = b.schema.properties ?? {};
  const ra = new Set(a.schema.required ?? []);
  const rb = new Set(b.schema.required ?? []);
  for (const [field, def] of Object.entries(pb)) {
    if (!(field in pa)) {
      const optional = !rb.has(field);
      const hasDefault = def && typeof def === 'object' && 'default' in def;
      if (optional && hasDefault) compatible.push(`Feld „${field}“ neu (optional, Default)`);
      else generation.push(`Feld „${field}“ neu ${optional ? 'ohne Default' : 'als Pflichtfeld'}`);
    } else if (!same(pa[field], def)) {
      if (typeLabel(pa[field]) !== typeLabel(def)) generation.push(`Typ von „${field}“: ${typeLabel(pa[field])} → ${typeLabel(def)}`);
      else generation.push(`Feld „${field}“ geändert`);
    }
  }
  for (const field of Object.keys(pa)) if (!(field in pb)) generation.push(`Feld „${field}“ entfernt`);
  for (const field of rb) if (!ra.has(field) && field in pa) generation.push(`„${field}“ wird Pflichtfeld`);
  for (const field of ra) if (!rb.has(field) && field in pb) generation.push(`„${field}“ nicht mehr Pflicht`);
  const keys = new Set([...Object.keys(a.schema), ...Object.keys(b.schema)]);
  for (const k of keys) {
    if (k === 'properties' || k === 'required') continue;
    if (!same(a.schema[k], b.schema[k])) generation.push(`Schema-Eigenschaft „${k}“ geändert`);
  }
  if (!same(a.indexes, b.indexes)) compatible.push('Indizes geändert');
  for (const k of META_KEYS) if (!same(a[k], b[k])) compatible.push(k === 'rights' ? 'Rechte geändert' : `${k} geändert`);
  return { compatible, generation };
}

/**
 * Vergleicht Basis- und Zielfassung. Liefert je Collection
 * { name, kind: 'unchanged'|'compatible'|'new'|'generation'|'removed', reasons[], needsMigration, migration }.
 * `baseVer` = base_app_ver: eine Migration gilt nur, wenn ihr from_ver dazu passt (sonst ist sie ein Rest einer
 * früheren Generation).
 */
export function classifyCollections(baseParsed, nextParsed, { baseVer = null } = {}) {
  const a = baseParsed?.collections ?? {};
  const b = nextParsed?.collections ?? {};
  const names = [...new Set([...Object.keys(a), ...Object.keys(b)])].sort();
  return names.map((name) => {
    if (!(name in a)) return { name, kind: 'new', reasons: ['Collection neu'], needsMigration: false, migration: null };
    if (!(name in b)) return { name, kind: 'removed', reasons: ['Collection entfernt (wird archiviert)'], needsMigration: false, migration: null };
    const { compatible, generation } = diffCollection(a[name], b[name]);
    const kind = generation.length ? 'generation' : compatible.length ? 'compatible' : 'unchanged';
    const m = nextParsed.migrations?.[name] ?? null;
    // Wie Kern-Stub cff5ba5 (§16.22 A2): migrate_fn ODER deklarative steps.
    const usable = m && (m.migrate_fn || Array.isArray(m.steps)) && (baseVer == null || String(m.from_ver) === String(baseVer));
    const migration = kind === 'generation' && usable ? m : null;
    return { name, kind, reasons: [...generation, ...compatible], needsMigration: kind === 'generation', migration };
  });
}

/** Blocker für „Veröffentlichen“: jede neue Generation braucht eine passende Migration (steps oder vorhandene migrate_fn-Datei). */
export function publishBlockers(diff, filePaths) {
  const have = new Set(filePaths);
  const out = [];
  for (const d of diff) {
    if (!d.needsMigration) continue;
    if (!d.migration) out.push({ name: d.name, reason: 'migration_missing' });
    else if (d.migration.migrate_fn && !have.has(d.migration.migrate_fn)) out.push({ name: d.name, reason: 'migration_file_missing', path: d.migration.migrate_fn });
  }
  return out;
}

export function migrationPath(name) { return `migrations/${name}.js`; }

export function migrationTemplate(name, baseVer, reasons) {
  return [
    `// Migration der Collection „${name}“ von ${baseVer} in die neue Generation (Architektur §16.4).`,
    `// Ausgeführt vom Migrationsrunner der App-Laufzeit (§16.9) je Dokument; Rückgabe = Dokument der neuen Generation.`,
    ...reasons.map((r) => `//   - ${r}`),
    'export default function migrate(doc) {',
    '  return { ...doc };',
    '}',
    '',
  ].join('\n');
}

/** Fügt den Migrationseintrag in den Dateitext ein (Formatierung: 2 Leerzeichen). */
export function withMigration(text, name, baseVer, path = migrationPath(name)) {
  const obj = text.trim() ? JSON.parse(text) : {};
  obj.migrations = { ...(obj.migrations ?? {}), [name]: { from_ver: baseVer, migrate_fn: path } };
  return `${JSON.stringify(obj, null, 2)}\n`;
}
